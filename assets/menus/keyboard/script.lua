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

local NON_REBIND_KEYS = {
    ShiftLeft = true, ShiftRight = true,
    ControlLeft = true, ControlRight = true,
    AltLeft = true, AltRight = true,
    SuperLeft = true, SuperRight = true,
    MouseMotion = true, MouseLeft = true,
    -- Right-click is the cancel gesture while remapping; MouseRight itself is
    -- still bindable by left-clicking the R-CLICK widget on the mouse diagram.
    MouseRight = true,
}

local function mods_equal(a, b)
    return (a.ctrl == true) == (b.ctrl == true)
        and (a.shift == true) == (b.shift == true)
        and (a.alt == true) == (b.alt == true)
        and (a.super == true) == (b.super == true)
end

local function has_any_mod(mods)
    return mods.ctrl == true or mods.shift == true or mods.alt == true or mods.super == true
end

local function copy_mods(mods)
    return {
        ctrl  = mods.ctrl == true,
        shift = mods.shift == true,
        alt   = mods.alt == true,
        super = mods.super == true,
    }
end

local function key_matches(a, b)
    if a == b then return true end
    if ALIASES[a] ~= nil then
        for _, alias in ipairs(ALIASES[a]) do
            if alias == b then return true end
        end
    end
    if ALIASES[b] ~= nil then
        for _, alias in ipairs(ALIASES[b]) do
            if alias == a then return true end
        end
    end
    return false
end

local function show_rebind_message(text, is_error)
    if rebind_hint_el == nil then return end
    dom.set_text(rebind_hint_el, text)
    if is_error == true then
        dom.remove_class(rebind_hint_el, "success")
        dom.add_class(rebind_hint_el, "error")
    elseif is_error == false then
        dom.remove_class(rebind_hint_el, "error")
        dom.add_class(rebind_hint_el, "success")
    else
        dom.remove_class(rebind_hint_el, "error")
        dom.remove_class(rebind_hint_el, "success")
    end
end

local function format_binding_chain(mods, key_str, label)
    local parts = {}
    if mods.ctrl  then table.insert(parts, "Ctrl") end
    if mods.shift then table.insert(parts, "Shift") end
    if mods.alt   then table.insert(parts, "Alt") end
    if mods.super then table.insert(parts, "Super") end
    table.insert(parts, key_str)
    local chain = table.concat(parts, " + ")
    if label ~= nil and label ~= "" then
        return chain .. " (" .. label .. ")"
    end
    return chain
end

local function effective_mods()
    return {
        ctrl  = physical_mods.ctrl  or pinned_mods.ctrl,
        shift = physical_mods.shift or pinned_mods.shift,
        alt   = physical_mods.alt   or pinned_mods.alt,
        super = physical_mods.super or pinned_mods.super,
    }
end

local function find_binding(code, mods)
    local fallback = nil
    for _, b in ipairs(all_bindings) do
        if mods_equal(b, mods) and key_matches(b.key, code) then
            if b.active ~= false then
                return b
            end
            if fallback == nil then
                fallback = b
            end
        end
    end
    return fallback
end

local function validate_placement(key, mods, is_movement, exclude_a, exclude_b)
    if is_movement then
        if has_any_mod(mods) then
            return "Movement keys cannot have modifier keys"
        end
        for _, b in ipairs(all_bindings) do
            if b ~= exclude_a and b ~= exclude_b
                and key_matches(b.key, key)
                and has_any_mod(b)
            then
                return "Cannot remap movement key to '" .. key .. "': key is part of hotkey chain '"
                    .. format_binding_chain(b, b.key, b.label) .. "'"
            end
        end
    elseif has_any_mod(mods) then
        for _, b in ipairs(all_bindings) do
            if b ~= exclude_a and b ~= exclude_b
                and b.is_movement
                and key_matches(b.key, key)
            then
                return "Cannot bind hotkey chain to movement key '" .. key .. "'"
            end
        end
    end
    return nil
end

local function refresh_keyboard()
    local mods = effective_mods()
    local is_other_view = mods.ctrl or mods.shift or mods.alt or mods.super
    dom.set_text(mod_indicator_el, format_mods_header(mods))

    local movement_keys = {}
    local lookup = {}
    for _, b in ipairs(all_bindings) do
        if b.is_movement then
            movement_keys[b.key] = true
            if ALIASES[b.key] ~= nil then
                for _, alias in ipairs(ALIASES[b.key]) do
                    movement_keys[alias] = true
                end
            end
        end

        if mods_equal(b, mods) then
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

    local is_remapping_movement = (remapping_source ~= nil and remapping_source.is_movement == true)

    for _, kw in ipairs(key_widgets) do
        if kw.fixed then
            goto continue_refresh
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
        kw.is_movement = is_movement

        local entry = lookup[kw.code]
        if entry == nil and ALIASES[kw.code] ~= nil then
            for _, alias in ipairs(ALIASES[kw.code]) do
                if lookup[alias] ~= nil then
                    entry = lookup[alias]
                    break
                end
            end
        end

        local is_source = (
            remapping_source ~= nil
            and remapping_source.code == kw.code
            and mods_equal(mods, remapping_source.mods)
        )

        local is_illegal_movement_target = false
        if is_remapping_movement and not is_source then
            local target_binding = find_binding(kw.code, mods)
            if target_binding ~= nil and target_binding.context ~= "gameplay" then
                is_illegal_movement_target = true
            elseif validate_placement(kw.code, mods, true, remapping_source.binding, target_binding) ~= nil then
                is_illegal_movement_target = true
            elseif target_binding ~= nil
                and validate_placement(
                    remapping_source.code,
                    remapping_source.mods,
                    target_binding.is_movement == true,
                    remapping_source.binding,
                    target_binding
                ) ~= nil
            then
                is_illegal_movement_target = true
            end
        end

        local is_locked = is_illegal_movement_target or (is_other_view and is_movement and (entry == nil))

        if is_locked then
            dom.remove_class(kw.el, "bound")
            dom.add_class(kw.el, "locked")
            if kw.desc_el ~= nil then
                dom.set_text(kw.desc_el, "")
            end
            if kw.lock_el ~= nil then
                dom.add_class(kw.lock_el, "visible")
            end
        elseif entry ~= nil then
            dom.add_class(kw.el, "bound")
            dom.remove_class(kw.el, "locked")
            if kw.desc_el ~= nil then
                dom.set_text(kw.desc_el, truncate_two_lines(entry.label, kw.max_cols))
            end
            if kw.lock_el ~= nil then
                dom.remove_class(kw.lock_el, "visible")
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

        if is_source then
            dom.add_class(kw.el, "remapping")
        else
            dom.remove_class(kw.el, "remapping")
        end

        ::continue_refresh::
    end
end

local function cancel_remapping()
    if remapping_source ~= nil then
        if remapping_source.widget ~= nil then
            dom.remove_class(remapping_source.widget.el, "remapping")
        end
        remapping_source = nil
    end
    show_rebind_message("", nil)
    refresh_keyboard()
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
    if source.code == "MouseMotion" or target_code == "MouseMotion" then
        cancel_remapping()
        show_rebind_message("Warning: Aim/Look cannot be rebound", true)
        return
    end

    if source.binding == nil or source.binding.context ~= "gameplay" then
        cancel_remapping()
        show_rebind_message("Warning: Only gameplay hotkeys can be remapped", true)
        return
    end

    local target_binding = find_binding(target_code, target_mods)
    if target_binding == source.binding then
        cancel_remapping()
        return
    end

    if target_binding ~= nil then
        if target_binding.context ~= "gameplay" then
            cancel_remapping()
            show_rebind_message("Cannot swap with non-gameplay keybind '" .. target_code .. "'", true)
            return
        end

        local err = validate_placement(
            target_code,
            target_mods,
            source.is_movement,
            source.binding,
            target_binding
        )
        if err ~= nil then
            cancel_remapping()
            show_rebind_message(err, true)
            return
        end

        local err_swap = validate_placement(
            source.code,
            source.mods,
            target_binding.is_movement == true,
            source.binding,
            target_binding
        )
        if err_swap ~= nil then
            cancel_remapping()
            show_rebind_message(err_swap, true)
            return
        end
    else
        local err = validate_placement(
            target_code,
            target_mods,
            source.is_movement,
            source.binding,
            nil
        )
        if err ~= nil then
            cancel_remapping()
            show_rebind_message(err, true)
            return
        end
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

local function create_lock_elements(parent_el)
    local lock_el = dom.create(parent_el, "node", { class = "lock-container" })
    local lock_icon = dom.create(lock_el, "node", { class = "lock-icon" })
    local lock_shackle = dom.create(lock_icon, "node", { class = "lock-shackle" })
    local lock_body = dom.create(lock_icon, "node", { class = "lock-body" })
    return lock_el, { lock_el, lock_icon, lock_shackle, lock_body }
end

function format_mods_header(mods)
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

function truncate_two_lines(text, max_cols)
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

function on_menu_ready(_, root_id)
    all_bindings = input.list_bindings()
    physical_mods = input.active_modifiers()
    remapping_source = nil

    mod_indicator_el = dom.get_by_id(root_id, "mod_indicator")
    rebind_hint_el = dom.get_by_id(root_id, "rebind_hint")
    local rows_container = dom.get_by_id(root_id, "keyboard_rows")
    local mouse_container = dom.get_by_id(root_id, "mouse_slots")
    dom.on(dom.get_by_id(root_id, "backdrop"), "click", "on_backdrop_click")
    dom.on(dom.get_by_id(root_id, "backdrop"), "right_click", "on_backdrop_click")
    dom.on(root_id, "right_click", "on_right_click")

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
                local lock_el, lock_nodes = create_lock_elements(bottom_el)

                local widget = {
                    code = key_def.code,
                    mod = key_def.mod,
                    is_movement = false,
                    el = key_el,
                    desc_el = desc_el,
                    lock_el = lock_el,
                    max_cols = WIDTH_CHARS[key_def.w] or 7,
                }
                table.insert(key_widgets, widget)

                local elements = { key_el, cap_el, bottom_el, desc_el }
                for _, ln in ipairs(lock_nodes) do
                    table.insert(elements, ln)
                end
                bind_widget_elements(widget, elements)
            end
        end
    end

    if mouse_container ~= nil then
        dom.clear(mouse_container)

        local function create_mouse_widget(parent, container_class, cap_class, cap_text, code, max_cols)
            local btn_el = dom.create(parent, "node", { class = container_class })
            local cap_el = dom.create(btn_el, "text", { class = cap_class, text = cap_text })
            local desc_el = dom.create(btn_el, "text", { class = "key-desc", text = "" })
            local lock_el, lock_nodes = create_lock_elements(btn_el)
            local w = {
                code = code,
                is_movement = false,
                el = btn_el,
                desc_el = desc_el,
                lock_el = lock_el,
                max_cols = max_cols,
            }
            table.insert(key_widgets, w)
            local elements = { btn_el, cap_el, desc_el }
            for _, ln in ipairs(lock_nodes) do
                table.insert(elements, ln)
            end
            bind_widget_elements(w, elements)
        end

        local mouse_device = dom.create(mouse_container, "node", { class = "mouse-device" })

        local thumb_col = dom.create(mouse_device, "node", { class = "mouse-thumb-col" })
        create_mouse_widget(thumb_col, "mouse-thumb-btn", "mouse-cap-small", "M5 FWD", "MouseForward", 5)
        create_mouse_widget(thumb_col, "mouse-thumb-btn", "mouse-cap-small", "M4 BACK", "MouseBack", 5)

        local body_el = dom.create(mouse_device, "node", { class = "mouse-body" })
        local top_row = dom.create(body_el, "node", { class = "mouse-top-row" })

        create_mouse_widget(top_row, "mouse-btn-left", "key-cap", "L-CLICK", "MouseLeft", 6)

        local wheel_col = dom.create(top_row, "node", { class = "mouse-wheel-col" })
        create_mouse_widget(wheel_col, "mouse-wheel-btn", "mouse-cap-small", "M-CLICK", "MouseMiddle", 5)
        create_mouse_widget(wheel_col, "mouse-wheel-scroll", "mouse-cap-small", "SCROLL", "MouseWheel", 5)

        create_mouse_widget(top_row, "mouse-btn-right", "key-cap", "R-CLICK", "MouseRight", 6)

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
        local current_mods = effective_mods()
        for _, key_code in ipairs(just_pressed_list) do
            if key_code == "Escape" then
                cancel_remapping()
                break
            elseif not NON_REBIND_KEYS[key_code] then
                if key_code == remapping_source.code and mods_equal(current_mods, remapping_source.mods) then
                    cancel_remapping()
                else
                    execute_remap(remapping_source, key_code, current_mods)
                end
                break
            end
        end
    end
end

function on_remap_result(success, message)
    all_bindings = input.list_bindings()
    refresh_keyboard()
    show_rebind_message(message, not success)
end

function on_key_widget_click(node_id)
    local kw = node_to_widget[node_id]
    if kw == nil then return end

    if kw.fixed or kw.code == "MouseMotion" then
        show_rebind_message("Warning: Aim/Look cannot be rebound", true)
        return
    end

    local current_mods = effective_mods()

    if remapping_source ~= nil then
        if kw.mod ~= nil then
            if kw.code == remapping_source.code and mods_equal(current_mods, remapping_source.mods) then
                cancel_remapping()
                return
            end
            pinned_mods[kw.mod] = not pinned_mods[kw.mod]
            refresh_keyboard()
            return
        end

        if kw.code == remapping_source.code and mods_equal(current_mods, remapping_source.mods) then
            cancel_remapping()
            return
        end

        execute_remap(remapping_source, kw.code, current_mods)
        return
    end

    if kw.mod ~= nil then
        pinned_mods[kw.mod] = not pinned_mods[kw.mod]
        refresh_keyboard()
        return
    end

    local b = find_binding(kw.code, current_mods)
    if b ~= nil then
        if b.context == "gameplay" then
            remapping_source = {
                code = kw.code,
                mods = copy_mods(current_mods),
                label = b.label,
                is_movement = (b.is_movement == true),
                binding = b,
                widget = kw,
            }
            refresh_keyboard()
            show_rebind_message(
                "Press any key/button or click a key to remap '" .. b.label .. "' (Esc / right-click to cancel)",
                nil
            )
        else
            show_rebind_message("Warning: Only gameplay hotkeys can be remapped", true)
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

-- Right-click anywhere in the menu cancels an in-progress rebind; outside a
-- rebind it does nothing (the backdrop handles its own right-click to close).
function on_right_click()
    if remapping_source ~= nil then
        cancel_remapping()
    end
end
