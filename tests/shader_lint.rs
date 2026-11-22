use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use wgsl_parse::parse_str;
use wgsl_parse::span::Spanned;
use wgsl_parse::syntax::{
    Attribute, CompoundStatement, DeclarationKind, Expression, ExpressionNode, ForStatement,
    Function, FunctionCall, FunctionCallStatement, GlobalDeclaration, LiteralExpression, Statement,
    TranslationUnit,
};

const SHADER_DIR: &str = "assets/shaders";
const SHARED_PRIMITIVES: [&str; 8] = [
    "hash21",
    "hash22",
    "hash33",
    "noise_and_grad",
    "smooth_noise2",
    "fbm2",
    "schlick_fresnel",
    "ggx_specular",
];
const DERIVATIVE_HELPERS: [&str; 3] = ["dpdx", "dpdy", "tile_mip"];
const SKY_TAP_BUDGET: u32 = 0;
const WATER_TAP_BUDGET: u32 = 0;
const TERRAIN_TAP_BUDGET: u32 = 57;

fn shader_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(SHADER_DIR)
        .expect("assets/shaders exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "wesl"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .wesl shaders found");
    files
}

fn read_shader(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn parse(path: &Path) -> TranslationUnit {
    let src = read_shader(path);
    parse_str(&src).unwrap_or_else(|e| panic!("{} failed to parse: {e}", path.display()))
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().to_string()
}

fn functions_in(tu: &TranslationUnit) -> Vec<Function> {
    tu.global_declarations
        .iter()
        .filter_map(|gd| match &**gd {
            GlobalDeclaration::Function(f) => Some(f.clone()),
            _ => None,
        })
        .collect()
}

fn is_texture_builtin(name: &str) -> bool {
    name.starts_with("textureSample")
        || name.starts_with("textureGather")
        || name == "textureLoad"
        || name == "textureFetch"
}

fn literal_int(expr: &ExpressionNode) -> Option<i64> {
    match &**expr {
        Expression::Literal(lit) => match lit {
            LiteralExpression::U32(v) => Some(*v as i64),
            LiteralExpression::I32(v) => Some(*v as i64),
            LiteralExpression::AbstractInt(v) => Some(*v),
            _ => None,
        },
        Expression::Parenthesized(p) => literal_int(&p.expression),
        _ => None,
    }
}

fn first_int_in(expr: &ExpressionNode) -> Option<i64> {
    if let Some(v) = literal_int(expr) {
        return Some(v);
    }
    match &**expr {
        Expression::Binary(b) => first_int_in(&b.left).or_else(|| first_int_in(&b.right)),
        Expression::Unary(u) => first_int_in(&u.operand),
        Expression::Parenthesized(p) => first_int_in(&p.expression),
        _ => None,
    }
}

fn for_trip_count(f: &ForStatement) -> u32 {
    f.condition
        .as_ref()
        .and_then(first_int_in)
        .filter(|v| *v > 0)
        .map(|v| v as u32)
        .unwrap_or(1)
}

struct TapCtx {
    user_fns: HashMap<String, Function>,
    memo: HashMap<String, u32>,
    visiting: HashSet<String>,
}

impl TapCtx {
    fn new(functions: Vec<Function>) -> Self {
        let user_fns = functions
            .into_iter()
            .map(|f| (f.ident.clone().name().to_string(), f))
            .collect();
        Self {
            user_fns,
            memo: HashMap::new(),
            visiting: HashSet::new(),
        }
    }

    fn compound(&mut self, cs: &CompoundStatement) -> u32 {
        cs.statements.iter().map(|s| self.statement(s)).sum()
    }

    fn statement(&mut self, stmt: &Spanned<Statement>) -> u32 {
        match &**stmt {
            Statement::Compound(c) => self.compound(c),
            Statement::If(i) => {
                let mut best = self.compound(&i.if_clause.body);
                best += self.expr_opt(&i.if_clause.expression);
                for c in &i.else_if_clauses {
                    best = best.max(self.compound(&c.body) + self.expr_opt(&c.expression));
                }
                if let Some(e) = &i.else_clause {
                    best = best.max(self.compound(&e.body));
                }
                best
            }
            Statement::Switch(s) => {
                let mut best = 0;
                for c in &s.clauses {
                    best = best.max(self.compound(&c.body));
                }
                best
            }
            Statement::For(f) => {
                let body = self.compound(&f.body);
                body * for_trip_count(f)
            }
            Statement::While(w) => self.compound(&w.body),
            Statement::Loop(l) => self.compound(&l.body),
            Statement::FunctionCall(fc) => self.call(&fc.call),
            Statement::Declaration(d) => d
                .initializer
                .as_ref()
                .map(|e| self.expr(e))
                .unwrap_or(0),
            Statement::Assignment(a) => self.expr(&a.rhs) + self.expr(&a.lhs),
            Statement::Return(r) => r.expression.as_ref().map(|e| self.expr(e)).unwrap_or(0),
            _ => 0,
        }
    }

    fn expr_opt(&mut self, expr: &ExpressionNode) -> u32 {
        self.expr(expr)
    }

    fn expr(&mut self, expr: &ExpressionNode) -> u32 {
        match &**expr {
            Expression::FunctionCall(fc) => self.call(fc),
            Expression::Binary(b) => self.expr(&b.left) + self.expr(&b.right),
            Expression::Unary(u) => self.expr(&u.operand),
            Expression::Parenthesized(p) => self.expr(&p.expression),
            Expression::Indexing(i) => self.expr(&i.base) + self.expr(&i.index),
            Expression::NamedComponent(n) => self.expr(&n.base),
            _ => 0,
        }
    }

    fn call(&mut self, call: &FunctionCall) -> u32 {
        let name = call.ty.ident.name().to_string();
        let mut total: u32 = 0;
        if is_texture_builtin(&name) {
            total += 1;
        } else if self.user_fns.contains_key(&name) && !self.visiting.contains(&name) {
            self.visiting.insert(name.clone());
            let cost = self.fn_taps(&name);
            self.visiting.remove(&name);
            total += cost;
        }
        for arg in &call.arguments {
            total += self.expr(arg);
        }
        total
    }

    fn fn_taps(&mut self, name: &str) -> u32 {
        if let Some(v) = self.memo.get(name) {
            return *v;
        }
        let body = self
            .user_fns
            .get(name)
            .map(|f| f.body.clone())
            .expect("function present");
        let cost = self.compound(&body);
        self.memo.insert(name.to_string(), cost);
        cost
    }
}

fn fragment_taps(tu: &TranslationUnit) -> u32 {
    let functions = functions_in(tu);
    let body = functions
        .iter()
        .find(|f| f.ident.name().as_str() == "fragment")
        .expect("fragment entry point")
        .body
        .clone();
    let mut ctx = TapCtx::new(functions);
    ctx.compound(&body)
}

fn walk_control(
    cs: &CompoundStatement,
    in_loop: bool,
    in_cond_or_loop: bool,
    fbm2_bound: Option<i64>,
    file: &str,
    errors: &mut Vec<String>,
) {
    for stmt in &cs.statements {
        match &**stmt {
            Statement::Compound(c) => walk_control(c, in_loop, in_cond_or_loop, fbm2_bound, file, errors),
            Statement::If(i) => {
                walk_control(&i.if_clause.body, in_loop, true, fbm2_bound, file, errors);
                for c in &i.else_if_clauses {
                    walk_control(&c.body, in_loop, true, fbm2_bound, file, errors);
                }
                if let Some(e) = &i.else_clause {
                    walk_control(&e.body, in_loop, true, fbm2_bound, file, errors);
                }
            }
            Statement::Switch(s) => {
                for c in &s.clauses {
                    walk_control(&c.body, in_loop, true, fbm2_bound, file, errors);
                }
            }
            Statement::For(f) => {
                walk_control(&f.body, true, true, fbm2_bound, file, errors);
            }
            Statement::While(w) => walk_control(&w.body, true, true, fbm2_bound, file, errors),
            Statement::Loop(l) => walk_control(&l.body, true, true, fbm2_bound, file, errors),
            Statement::Declaration(d) => {
                if let Some(e) = &d.initializer {
                    check_expr_calls(e, in_loop, in_cond_or_loop, fbm2_bound, file, errors);
                }
            }
            Statement::Assignment(a) => {
                check_expr_calls(&a.rhs, in_loop, in_cond_or_loop, fbm2_bound, file, errors);
            }
            Statement::FunctionCall(fc) => {
                check_expr_calls_node(fc, in_loop, in_cond_or_loop, fbm2_bound, file, errors)
            }
            Statement::Return(r) => {
                if let Some(e) = &r.expression {
                    check_expr_calls(e, in_loop, in_cond_or_loop, fbm2_bound, file, errors);
                }
            }
            _ => {}
        }
    }
}

fn check_expr_calls_node(
    fc: &FunctionCallStatement,
    in_loop: bool,
    in_cond_or_loop: bool,
    fbm2_bound: Option<i64>,
    file: &str,
    errors: &mut Vec<String>,
) {
    check_call(&fc.call, in_loop, in_cond_or_loop, fbm2_bound, file, errors);
}

fn check_call(
    call: &FunctionCall,
    in_loop: bool,
    in_cond_or_loop: bool,
    fbm2_bound: Option<i64>,
    file: &str,
    errors: &mut Vec<String>,
) {
    let name = call.ty.ident.name().to_string();
    if name == "fbm2" {
        if in_loop {
            errors.push(format!("{file}: fbm2 called inside a loop body"));
        }
        if let (Some(bound), Some(oct)) = (fbm2_bound, call.arguments.get(1).and_then(literal_int)) {
            if oct > bound {
                errors.push(format!(
                    "{file}: fbm2 octaves {oct} exceeds bound {bound}"
                ));
            }
        }
    }
    if DERIVATIVE_HELPERS.contains(&name.as_str()) && in_cond_or_loop {
        errors.push(format!(
            "{file}: {name} called inside a conditional or loop block"
        ));
    }
    for arg in &call.arguments {
        check_expr_calls(arg, in_loop, in_cond_or_loop, fbm2_bound, file, errors);
    }
}

fn check_expr_calls(
    expr: &ExpressionNode,
    in_loop: bool,
    in_cond_or_loop: bool,
    fbm2_bound: Option<i64>,
    file: &str,
    errors: &mut Vec<String>,
) {
    match &**expr {
        Expression::FunctionCall(fc) => {
            check_call(fc, in_loop, in_cond_or_loop, fbm2_bound, file, errors)
        }
        Expression::Binary(b) => {
            check_expr_calls(&b.left, in_loop, in_cond_or_loop, fbm2_bound, file, errors);
            check_expr_calls(&b.right, in_loop, in_cond_or_loop, fbm2_bound, file, errors);
        }
        Expression::Unary(u) => {
            check_expr_calls(&u.operand, in_loop, in_cond_or_loop, fbm2_bound, file, errors)
        }
        Expression::Parenthesized(p) => {
            check_expr_calls(&p.expression, in_loop, in_cond_or_loop, fbm2_bound, file, errors)
        }
        Expression::Indexing(i) => {
            check_expr_calls(&i.base, in_loop, in_cond_or_loop, fbm2_bound, file, errors);
            check_expr_calls(&i.index, in_loop, in_cond_or_loop, fbm2_bound, file, errors);
        }
        Expression::NamedComponent(n) => {
            check_expr_calls(&n.base, in_loop, in_cond_or_loop, fbm2_bound, file, errors)
        }
        _ => {}
    }
}

fn fbm2_bound_for(file: &str) -> Option<i64> {
    match file {
        "terrain_material.wesl" | "water_material.wesl" => Some(2),
        "sky_material.wesl" => Some(4),
        _ => None,
    }
}

#[test]
fn wesl_all_shaders_parse() {
    for path in shader_files() {
        let _ = parse(&path);
    }
}

#[test]
fn shaderutils_is_stateless() {
    let path = Path::new(SHADER_DIR).join("shaderutils.wesl");
    let tu = parse(&path);
    let mut errors = Vec::new();
    for gd in &tu.global_declarations {
        if let GlobalDeclaration::Declaration(d) = &**gd {
            if matches!(d.kind, DeclarationKind::Var(_)) {
                errors.push(format!(
                    "shaderutils.wesl: module-level var `{}`",
                    d.ident.name()
                ));
            }
            for attr in &d.attributes {
                if matches!(&**attr, Attribute::Group(_) | Attribute::Binding(_)) {
                    errors.push("shaderutils.wesl: @group/@binding attribute found".to_string());
                }
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn no_duplicate_shared_primitives() {
    let mut errors = Vec::new();
    for path in shader_files() {
        let name = file_name(&path);
        if name == "shaderutils.wesl" {
            continue;
        }
        let tu = parse(&path);
        for f in functions_in(&tu) {
            let fname = f.ident.name().to_string();
            if SHARED_PRIMITIVES.contains(&fname.as_str()) {
                errors.push(format!("{name}: redefines shared primitive `{fname}`"));
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn noise_and_loop_budget() {
    let mut errors = Vec::new();
    for path in shader_files() {
        let name = file_name(&path);
        let bound = fbm2_bound_for(&name);
        let tu = parse(&path);
        for f in functions_in(&tu) {
            walk_control(&f.body, false, false, bound, &name, &mut errors);
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn texture_tap_budget() {
    let mut errors = Vec::new();
    for path in shader_files() {
        let name = file_name(&path);
        let budget = match name.as_str() {
            "sky_material.wesl" => SKY_TAP_BUDGET,
            "water_material.wesl" => WATER_TAP_BUDGET,
            "terrain_material.wesl" => TERRAIN_TAP_BUDGET,
            _ => continue,
        };
        let tu = parse(&path);
        let taps = fragment_taps(&tu);
        if taps > budget {
            errors.push(format!(
                "{name}: fragment worst-case texture taps {taps} exceeds budget {budget}"
            ));
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
