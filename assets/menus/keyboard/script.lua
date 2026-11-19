local dom   = import_namespace("dom")
local input = import_namespace("input")
local menus = import_namespace("menus")

function on_dom_event(handler, node, payload)
    local fn = _G[handler]
    if fn ~= nil then
        fn(node, payload)
    end
end

local LAYOUT = {
    {
        { code = "Escape", label = "ESC", w = "w-1" },
        { spacer = true },
        { code = "F1",  label = "F1",  w = "w-1" }, { code = "F2",  label = "F2",  w = "w-1" },
        { code = "F3",  label = "F3",  w = "w-1" }, { code = "F4",  label = "F4",  w = "w-1" },
        { spacer = true },
        { code = "F5",  label = "F5",  w = "w-1" }, { code = "F6",  label = "F6",  w = "w-1" },
        { code = "F7",  label = "F7",  w = "w-1" }, { code = "F8",  label = "F8",  w = "w-1" },
        { spacer = true },
        { code = "F9",  label = "F9",  w = "w-1" }, { code = "F10", label = "F10", w = "w-1" },
        { code = "F11", label = "F11", w = "w-1" }, { code = "F12", label = "F12", w = "w-1" },
    },
    {
        { code = "Backquote", label = "`", w = "w-1" },
        { code = "Digit1", label = "1", w = "w-1" }, { code = "Digit2", label = "2", w = "w-1" },
        { code = "Digit3", label = "3", w = "w-1" }, { code = "Digit4", label = "4", w = "w-1" },
        { code = "Digit5", label = "5", w = "w-1" }, { code = "Digit6", label = "6", w = "w-1" },
        { code = "Digit7", label = "7", w = "w-1" }, { code = "Digit8", label = "8", w = "w-1" },
        { code = "Digit9", label = "9", w = "w-1" }, { code = "Digit0", label = "0", w = "w-1" },
        { code = "Minus",  label = "-", w = "w-1" }, { code = "Equal",  label = "=", w = "w-1" },
        { code = "Backspace", label = "BACK", w = "w-2" },
    },
    {
        { code = "Tab",  label = "TAB", w = "w-15" },
        { code = "KeyQ", label = "Q", w = "w-1" }, { code = "KeyW", label = "W", w = "w-1" },
        { code = "KeyE", label = "E", w = "w-1" }, { code = "KeyR", label = "R", w = "w-1" },
        { code = "KeyT", label = "T", w = "w-1" }, { code = "KeyY", label = "Y", w = "w-1" },
        { code = "KeyU", label = "U", w = "w-1" }, { code = "KeyI", label = "I", w = "w-1" },
        { code = "KeyO", label = "O", w = "w-1" }, { code = "KeyP", label = "P", w = "w-1" },
        { code = "BracketLeft",  label = "[", w = "w-1" },
        { code = "BracketRight", label = "]", w = "w-1" },
        { code = "Backslash",    label = "\\", w = "w-15" },
    },
    {
        { code = "CapsLock", label = "CAPS", w = "w-175" },
        { code = "KeyA", label = "A", w = "w-1" }, { code = "KeyS", label = "S", w = "w-1" },
        { code = "KeyD", label = "D", w = "w-1" }, { code = "KeyF", label = "F", w = "w-1" },
        { code = "KeyG", label = "G", w = "w-1" }, { code = "KeyH", label = "H", w = "w-1" },
        { code = "KeyJ", label = "J", w = "w-1" }, { code = "KeyK", label = "K", w = "w-1" },
        { code = "KeyL", label = "L", w = "w-1" },
        { code = "Semicolon", label = ";", w = "w-1" },
        { code = "Quote",     label = "'", w = "w-1" },
        { code = "Enter",     label = "ENTER", w = "w-225" },
    },
    {
        { code = "ShiftLeft",  label = "SHIFT", w = "w-225", mod = "shift" },
        { code = "KeyZ", label = "Z", w = "w-1" }, { code = "KeyX", label = "X", w = "w-1" },
        { code = "KeyC", label = "C", w = "w-1" }, { code = "KeyV", label = "V", w = "w-1" },
        { code = "KeyB", label = "B", w = "w-1" }, { code = "KeyN", label = "N", w = "w-1" },
        { code = "KeyM", label = "M", w = "w-1" },
        { code = "Comma",  label = ",", w = "w-1" },
        { code = "Period", label = ".", w = "w-1" },
        { code = "Slash",  label = "/", w = "w-1" },
        { code = "ShiftRight", label = "SHIFT", w = "w-275", mod = "shift" },
    },
    {
        { code = "ControlLeft",  label = "CTRL", w = "w-125", mod = "ctrl" },
        { code = "SuperLeft",    label = "WIN",  w = "w-125", mod = "super" },
        { code = "AltLeft",      label = "ALT",  w = "w-125", mod = "alt" },
        { code = "Space",        label = "SPACE", w = "w-625" },
        { code = "AltRight",     label = "ALT",  w = "w-125", mod = "alt" },
        { code = "SuperRight",   label = "WIN",  w = "w-125", mod = "super" },
        { code = "ControlRight", label = "CTRL", w = "w-125", mod = "ctrl" },
    },
}

local ALIASES = {
    ["MouseBack"]    = { "MouseBack", "Back", "MouseButton4" },
    ["MouseForward"] = { "MouseForward", "Forward", "MouseButton5" },
    ["MouseLeft"]    = { "MouseLeft", "Left", "MouseButton1" },
    ["MouseRight"]   = { "MouseRight", "Right", "MouseButton2" },
    ["MouseMiddle"]  = { "MouseMiddle", "Middle", "MouseButton3" },
}

local WIDTH_CHARS = {
    ["w-1"] = 7, ["w-125"] = 9, ["w-15"] = 11, ["w-175"] = 13,
    ["w-2"] = 16, ["w-225"] = 18, ["w-275"] = 22, ["w-625"] = 48,
}

local all_bindings = {}
local key_widgets = {}
local node_to_widget = {}
local mod_indicator_el = nil
local rebind_hint_el = nil

local physical_mods = { ctrl = false, shift = false, alt = false, super = false }
local pinned_mods   = { ctrl = false, shift = false, alt = false, super = false }

local remapping_source = nil

local function effective_mods()
    return {
        ctrl  = physical_mods.ctrl  or pinned_mods.ctrl,
        shift = physical_mods.shift or pinned_mods.shift,
        alt   = physical_mods.alt   or pinned_mods.alt,
        super = physical_mods.super or pinned_mods.super,
    }
end

local function get_movement_keys()
    local mv = {}
    for _, b in ipairs(all_bindings) do
        if b.is_movement then
            mv[b.key] = true
            if ALIASES[b.key] ~= nil then
                for _, alias in ipairs(ALIASES[b.key]) do
                    mv[alias] = true
                end
            end
        end
    end
    return mv
end

local function truncate_two_lines(text, max_cols)
    if text == nil or text == "" then
        return ""
    end

    local words = {}
    for w in string.gmatch(text, "%S+") do
        table.insert(words, w)
    end
    if #words == 0 then
        return ""
    end

    local lines = { "" }
    local line_idx = 1

    for _, word in ipairs(words) do
        local cur = lines[line_idx]
        local candidate = (cur == "") and word or (cur .. " " .. word)
        if #candidate <= max_cols then
            lines[line_idx] = candidate
        elseif line_idx == 1 then
            if cur == "" then
                lines[1] = string.sub(word, 1, max_cols)
                local rest = string.sub(word, max_cols + 1)
                if #rest > 0 then
                    line_idx = 2
                    lines[2] = rest
                    if #lines[2] > max_cols then
                        lines[2] = string.sub(lines[2], 1, math.max(1, max_cols - 3)) .. "..."
                        return table.concat(lines, "\n")
                    end
                end
            else
                line_idx = 2
                lines[2] = word
                if #lines[2] > max_cols then
                    lines[2] = string.sub(lines[2], 1, math.max(1, max_cols - 3)) .. "..."
                    return table.concat(lines, "\n")
                end
            end
        else
            local base = cur
            local room = max_cols - 3
            if #base > room then
                base = string.sub(base, 1, math.max(1, room))
            end
            lines[2] = base .. "..."
            return table.concat(lines, "\n")
        end
    end

    return table.concat(lines, "\n")
end

local function format_mods_header(mods)
    local parts = {}
    if mods.ctrl  then table.insert(parts, "Ctrl") end
    if mods.shift then table.insert(parts, "Shift") end
    if mods.alt   then table.insert(parts, "Alt") end
    if mods.super then table.insert(parts, "Super") end
    if #parts == 0 then
        return "Modifiers: None"
    end
    return "Modifiers: " .. table.concat(parts, " + ")
end

local function find_binding(code, mods)
    for _, b in ipairs(all_bindings) do
        if b.ctrl == mods.ctrl
            and b.shift == mods.shift
            and b.alt == mods.alt
            and b.super == mods.super
        then
            if b.key == code then
                return b
            end
            if ALIASES[code] ~= nil then
                for _, alias in ipairs(ALIASES[code]) do
                    if b.key == alias then
                        return b
                    end
                end
            end
        end
    end
    return nil
end

local function cancel_remapping()
    if remapping_source ~= nil then
        if remapping_source.widget ~= nil then
            dom.remove_class(remapping_source.widget.el, "remapping")
        end
        remapping_source = nil
    end
    if rebind_hint_el ~= nil then
        dom.set_text(rebind_hint_el, "")
        dom.remove_class(rebind_hint_el, "error")
        dom.remove_class(rebind_hint_el, "success")
    end
end

local function refresh_keyboard()
    local mods = effective_mods()
    local is_other_view = mods.ctrl or mods.shift or mods.alt or mods.super
    local movement_keys = get_movement_keys()
    dom.set_text(mod_indicator_el, format_mods_header(mods))

    local lookup = {}
    for _, b in ipairs(all_bindings) do
        if b.ctrl == mods.ctrl
            and b.shift == mods.shift
            and b.alt == mods.alt
            and b.super == mods.super
        then
            local is_active = (b.active ~= false)
            local existing = lookup[b.key]
            if existing == nil then
                lookup[b.key] = { label = b.label, active = is_active, context = b.context }
            elseif is_active and not existing.active then
                lookup[b.key] = { label = b.label, active = true, context = b.context }
            elseif is_active == existing.active then
                existing.label = existing.label .. " / " .. b.label
            end
        end
    end

    for _, kw in ipairs(key_widgets) do
        if kw.fixed then
            goto continue_refresh
        end

        local entry = lookup[kw.code]
        if entry == nil and ALIASES[kw.code] ~= nil then
            for _, alias in ipairs(ALIASES[kw.code]) do
                if lookup[alias] ~= nil then
                    entry = lookup[alias]
                    break
                end
            end
        end

        local is_movement = movement_keys[kw.code] == true
        if not is_movement and ALIASES[kw.code] ~= nil then
            for _, alias in ipairs(ALIASES[kw.code]) do
                if movement_keys[alias] then
                    is_movement = true
                    break
                end
            end
        end

        local is_locked_movement = is_other_view and is_movement and (entry == nil)

        if entry ~= nil then
            dom.add_class(kw.el, "bound")
            dom.remove_class(kw.el, "locked")
            if kw.desc_el ~= nil then
                dom.set_text(kw.desc_el, truncate_two_lines(entry.label, kw.max_cols))
            end
            if kw.lock_el ~= nil then
                dom.remove_class(kw.lock_el, "visible")
            end
        elseif is_locked_movement then
            dom.remove_class(kw.el, "bound")
            dom.add_class(kw.el, "locked")
            if kw.desc_el ~= nil then
                dom.set_text(kw.desc_el, "")
            end
            if kw.lock_el ~= nil then
                dom.add_class(kw.lock_el, "visible")
            end
        else
            dom.remove_class(kw.el, "bound")
            dom.remove_class(kw.el, "locked")
            if kw.desc_el ~= nil then
                dom.set_text(kw.desc_el, "")
            end
            if kw.lock_el ~= nil then
                dom.remove_class(kw.lock_el, "visible")
            end
        end

        if kw.mod ~= nil then
            if mods[kw.mod] then
                dom.add_class(kw.el, "mod-held")
            else
                dom.remove_class(kw.el, "mod-held")
            end
        end

        if remapping_source ~= nil and remapping_source.code == kw.code then
            dom.add_class(kw.el, "remapping")
        else
            dom.remove_class(kw.el, "remapping")
        end

        ::continue_refresh::
    end
end

local function update_pressed_widgets(pressed_list)
    local pressed_set = {}
    if pressed_list ~= nil then
        for _, code in ipairs(pressed_list) do
            pressed_set[code] = true
        end
    end

    for _, kw in ipairs(key_widgets) do
        if kw.fixed or kw.code == "MouseMotion" then
            goto continue_pressed
        end

        local is_p = pressed_set[kw.code] == true
        if not is_p and ALIASES[kw.code] ~= nil then
            for _, alias in ipairs(ALIASES[kw.code]) do
                if pressed_set[alias] then
                    is_p = true
                    break
                end
            end
        end

        if is_p then
            dom.add_class(kw.el, "pressed")
        else
            dom.remove_class(kw.el, "pressed")
        end

        ::continue_pressed::
    end
end

local function execute_remap(source, target_code, target_mods)
    if target_code == "MouseMotion" then
        if rebind_hint_el ~= nil then
            dom.set_text(rebind_hint_el, "Warning: Aim/Look cannot be rebound")
            dom.add_class(rebind_hint_el, "error")
        end
        return
    end

    cancel_remapping()
    input.remap(source.code, source.mods, target_code, target_mods)
end

local function bind_widget_elements(widget, elements)
    for _, el in ipairs(elements) do
        if el ~= nil then
            node_to_widget[el] = widget
            dom.on(el, "click", "on_key_widget_click")
        end
    end
end

function on_menu_ready(_, root_id)
    all_bindings = input.list_bindings()
    physical_mods = input.active_modifiers()
    remapping_source = nil

    mod_indicator_el = dom.get_by_id(root_id, "mod_indicator")
    rebind_hint_el = dom.get_by_id(root_id, "rebind_hint")
    local rows_container = dom.get_by_id(root_id, "keyboard_rows")
    local mouse_container = dom.get_by_id(root_id, "mouse_slots")
    dom.on(dom.get_by_id(root_id, "backdrop"), "click", "on_backdrop_click")

    dom.clear(rows_container)
    key_widgets = {}
    node_to_widget = {}

    for _, row_def in ipairs(LAYOUT) do
        local row_el = dom.create(rows_container, "node", { class = "kb-row" })
        for _, key_def in ipairs(row_def) do
            if key_def.spacer then
                dom.create(row_el, "node", { class = "spacer" })
            else
                local key_el = dom.create(row_el, "node", { class = "key " .. key_def.w })
                local cap_el = dom.create(key_el, "text", { class = "key-cap", text = key_def.label })

                local bottom_el = dom.create(key_el, "node", { class = "key-bottom" })
                local desc_el = dom.create(bottom_el, "text", { class = "key-desc", text = "" })

                local lock_el = dom.create(bottom_el, "node", { class = "lock-container" })
                local lock_icon = dom.create(lock_el, "node", { class = "lock-icon" })
                local lock_shackle = dom.create(lock_icon, "node", { class = "lock-shackle" })
                local lock_body = dom.create(lock_icon, "node", { class = "lock-body" })
                local lock_text = dom.create(lock_el, "text", { class = "lock-text", text = "LOCKED" })

                local widget = {
                    code = key_def.code,
                    mod = key_def.mod,
                    el = key_el,
                    desc_el = desc_el,
                    lock_el = lock_el,
                    max_cols = WIDTH_CHARS[key_def.w] or 7,
                }
                table.insert(key_widgets, widget)

                bind_widget_elements(widget, {
                    key_el, cap_el, bottom_el, desc_el,
                    lock_el, lock_icon, lock_shackle, lock_body, lock_text
                })
            end
        end
    end

    if mouse_container ~= nil then
        dom.clear(mouse_container)

        local mouse_device = dom.create(mouse_container, "node", { class = "mouse-device" })

        local thumb_col = dom.create(mouse_device, "node", { class = "mouse-thumb-col" })
        local m5_el = dom.create(thumb_col, "node", { class = "mouse-thumb-btn" })
        local m5_cap = dom.create(m5_el, "text", { class = "mouse-cap-small", text = "M5 FWD" })
        local m5_desc = dom.create(m5_el, "text", { class = "key-desc", text = "" })
        local w_m5 = { code = "MouseForward", el = m5_el, desc_el = m5_desc, max_cols = 5 }
        table.insert(key_widgets, w_m5)
        bind_widget_elements(w_m5, { m5_el, m5_cap, m5_desc })

        local m4_el = dom.create(thumb_col, "node", { class = "mouse-thumb-btn" })
        local m4_cap = dom.create(m4_el, "text", { class = "mouse-cap-small", text = "M4 BACK" })
        local m4_desc = dom.create(m4_el, "text", { class = "key-desc", text = "" })
        local w_m4 = { code = "MouseBack", el = m4_el, desc_el = m4_desc, max_cols = 5 }
        table.insert(key_widgets, w_m4)
        bind_widget_elements(w_m4, { m4_el, m4_cap, m4_desc })

        local body_el = dom.create(mouse_device, "node", { class = "mouse-body" })
        local top_row = dom.create(body_el, "node", { class = "mouse-top-row" })

        local ml_el = dom.create(top_row, "node", { class = "mouse-btn-left" })
        local ml_cap = dom.create(ml_el, "text", { class = "key-cap", text = "L-CLICK" })
        local ml_desc = dom.create(ml_el, "text", { class = "key-desc", text = "" })
        local w_ml = { code = "MouseLeft", el = ml_el, desc_el = ml_desc, max_cols = 6 }
        table.insert(key_widgets, w_ml)
        bind_widget_elements(w_ml, { ml_el, ml_cap, ml_desc })

        local wheel_col = dom.create(top_row, "node", { class = "mouse-wheel-col" })
        local mm_el = dom.create(wheel_col, "node", { class = "mouse-wheel-btn" })
        local mm_cap = dom.create(mm_el, "text", { class = "mouse-cap-small", text = "M-CLICK" })
        local mm_desc = dom.create(mm_el, "text", { class = "key-desc", text = "" })
        local w_mm = { code = "MouseMiddle", el = mm_el, desc_el = mm_desc, max_cols = 5 }
        table.insert(key_widgets, w_mm)
        bind_widget_elements(w_mm, { mm_el, mm_cap, mm_desc })

        local mw_el = dom.create(wheel_col, "node", { class = "mouse-wheel-scroll" })
        local mw_cap = dom.create(mw_el, "text", { class = "mouse-cap-small", text = "SCROLL" })
        local mw_desc = dom.create(mw_el, "text", { class = "key-desc", text = "" })
        local w_mw = { code = "MouseWheel", el = mw_el, desc_el = mw_desc, max_cols = 5 }
        table.insert(key_widgets, w_mw)
        bind_widget_elements(w_mw, { mw_el, mw_cap, mw_desc })

        local mr_el = dom.create(top_row, "node", { class = "mouse-btn-right" })
        local mr_cap = dom.create(mr_el, "text", { class = "key-cap", text = "R-CLICK" })
        local mr_desc = dom.create(mr_el, "text", { class = "key-desc", text = "" })
        local w_mr = { code = "MouseRight", el = mr_el, desc_el = mr_desc, max_cols = 6 }
        table.insert(key_widgets, w_mr)
        bind_widget_elements(w_mr, { mr_el, mr_cap, mr_desc })

        -- Palm / Aim & Look (Fixed permanent lock center)
        local palm_el = dom.create(body_el, "node", { class = "mouse-palm" })
        local palm_cap = dom.create(palm_el, "text", { class = "key-cap", text = "AIM / LOOK" })
        local palm_lock = dom.create(palm_el, "node", { class = "palm-lock-center" })
        local palm_lock_icon = dom.create(palm_lock, "node", { class = "lock-icon" })
        dom.create(palm_lock_icon, "node", { class = "lock-shackle" })
        dom.create(palm_lock_icon, "node", { class = "lock-body" })
        dom.create(palm_lock, "text", { class = "lock-text", text = "LOCKED" })
        dom.create(palm_el, "node", { class = "spacer" })

        local w_palm = { code = "MouseMotion", el = palm_el, desc_el = nil, max_cols = 14, fixed = true }
        table.insert(key_widgets, w_palm)
        bind_widget_elements(w_palm, { palm_el, palm_cap, palm_lock })
    end

    refresh_keyboard()
end

function on_modifiers_changed(mods, pressed_list, just_pressed_list)
    local mods_changed = (physical_mods.ctrl ~= mods.ctrl)
        or (physical_mods.shift ~= mods.shift)
        or (physical_mods.alt ~= mods.alt)
        or (physical_mods.super ~= mods.super)
    physical_mods = mods

    if mods_changed then
        refresh_keyboard()
    end

    update_pressed_widgets(pressed_list)

    if remapping_source ~= nil and just_pressed_list ~= nil then
        for _, key_code in ipairs(just_pressed_list) do
            if key_code == "Escape" then
                cancel_remapping()
                break
            elseif key_code ~= "ShiftLeft"
                and key_code ~= "ShiftRight"
                and key_code ~= "ControlLeft"
                and key_code ~= "ControlRight"
                and key_code ~= "AltLeft"
                and key_code ~= "AltRight"
                and key_code ~= "SuperLeft"
                and key_code ~= "SuperRight"
                and key_code ~= "MouseMotion"
                and key_code ~= "MouseLeft"
            then
                execute_remap(remapping_source, key_code, effective_mods())
                break
            end
        end
    end
end

function on_remap_result(success, message)
    all_bindings = input.list_bindings()
    refresh_keyboard()

    if rebind_hint_el ~= nil then
        dom.set_text(rebind_hint_el, message)
        if success then
            dom.remove_class(rebind_hint_el, "error")
            dom.add_class(rebind_hint_el, "success")
        else
            dom.remove_class(rebind_hint_el, "success")
            dom.add_class(rebind_hint_el, "error")
        end
    end
end

function on_key_widget_click(node_id)
    local kw = node_to_widget[node_id]
    if kw == nil then return end

    if kw.fixed or kw.code == "MouseMotion" then
        if rebind_hint_el ~= nil then
            dom.set_text(rebind_hint_el, "Warning: Aim/Look cannot be rebound")
            dom.add_class(rebind_hint_el, "error")
            dom.remove_class(rebind_hint_el, "success")
        end
        return
    end

    if remapping_source ~= nil then
        if kw.mod ~= nil and kw.code == remapping_source.code then
            cancel_remapping()
            return
        end

        if kw.mod ~= nil then
            pinned_mods[kw.mod] = not pinned_mods[kw.mod]
            refresh_keyboard()
            return
        end

        if kw.code == remapping_source.code then
            cancel_remapping()
            return
        end

        execute_remap(remapping_source, kw.code, effective_mods())
        return
    end

    if kw.mod ~= nil then
        pinned_mods[kw.mod] = not pinned_mods[kw.mod]
        refresh_keyboard()
        return
    end

    local current_mods = effective_mods()
    local b = find_binding(kw.code, current_mods)
    if b ~= nil then
        if b.context == "gameplay" then
            remapping_source = {
                code = kw.code,
                mods = current_mods,
                label = b.label,
                widget = kw,
            }
            dom.add_class(kw.el, "remapping")
            if rebind_hint_el ~= nil then
                dom.set_text(
                    rebind_hint_el,
                    "Press any key/button or click a key to remap '" .. b.label .. "' (Esc to cancel)"
                )
                dom.remove_class(rebind_hint_el, "error")
                dom.remove_class(rebind_hint_el, "success")
            end
        else
            if rebind_hint_el ~= nil then
                dom.set_text(rebind_hint_el, "Warning: Only gameplay hotkeys can be remapped")
                dom.add_class(rebind_hint_el, "error")
            end
        end
    end
end

function on_backdrop_click()
    if remapping_source ~= nil then
        cancel_remapping()
    else
        menus.close_top()
    end
end