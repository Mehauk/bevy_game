local dom      = import_namespace("dom")
local commands = import_namespace("commands")
local menus    = import_namespace("menus")
local state    = import_namespace("state")

function on_dom_event(handler, node, payload)
    local fn = _G[handler]
    if fn ~= nil then
        fn(node, payload)
    end
end

local MAX_ROWS = 100     -- cap on filtered entries kept
local VISIBLE_ROWS = 5   -- rows that fit in .palette-list; keep in sync with style.css
local current_menu_name = nil
local all_commands = {}
local visible = {}
local selected_index = nil
local current_raw_input = ""

local list_el = nil
local search_el = nil
local preview_el = nil

local function fuzzy_match(candidate, query)
    if query == "" then
        return 0, {}
    end

    local c_lower = string.lower(candidate)
    local q_lower = string.lower(query)
    local matched = {}

    if c_lower == q_lower then
        for i = 1, #candidate do
            matched[i] = true
        end
        return 10000, matched
    end

    local start_pos, end_pos = string.find(c_lower, q_lower, 1, true)
    if start_pos ~= nil then
        for i = start_pos, end_pos do
            matched[i] = true
        end
        local bonus = (start_pos == 1) and 500 or 0
        return 5000 + bonus - #c_lower, matched
    end

    local qi = 1
    local qlen = #q_lower
    local score = 0
    local prev_matched = false
    local prev_char = nil

    for i = 1, #c_lower do
        if qi > qlen then
            break
        end
        local c = string.sub(c_lower, i, i)
        local q = string.sub(q_lower, qi, qi)
        if c == q then
            local at_boundary = prev_char == nil
                or prev_char == ">"
                or prev_char == "_"
                or prev_char == "-"
                or prev_char == " "
                or prev_char == ":"
            score = score + 10
            if at_boundary then score = score + 25 end
            if prev_matched then score = score + 15 end
            matched[i] = true
            prev_matched = true
            qi = qi + 1
        else
            prev_matched = false
        end
        prev_char = c
    end

    if qi > qlen then
        return score - #c_lower, matched
    end
    return nil, nil
end

local function segments_from_matches(candidate, matched)
    if matched == nil or next(matched) == nil then
        return { { text = candidate, matched = false } }
    end

    local segments = {}
    local current_text = ""
    local current_matched = nil

    for i = 1, #candidate do
        local m = matched[i] == true
        local ch = string.sub(candidate, i, i)
        if current_matched == nil then
            current_matched = m
            current_text = ch
        elseif current_matched == m then
            current_text = current_text .. ch
        else
            table.insert(segments, { text = current_text, matched = current_matched })
            current_matched = m
            current_text = ch
        end
    end
    if #current_text > 0 then
        table.insert(segments, { text = current_text, matched = current_matched })
    end

    return segments
end

-- Renders only the window of rows that fits in .palette-list. The window is
-- positioned so the selected row sits at the bottom of the list (Skyrim-console
-- style) whenever enough rows remain above it to scroll.
local function render_window()
    dom.clear(list_el)

    local count = #visible
    local first = 1
    if selected_index ~= nil then
        local max_first = math.max(count - VISIBLE_ROWS + 1, 1)
        first = math.min(math.max(selected_index - VISIBLE_ROWS + 1, 1), max_first)
    end
    local last = math.min(count, first + VISIBLE_ROWS - 1)

    for i = first, last do
        local entry = visible[i]
        local row = dom.create(list_el, "node", {
            class = (i == selected_index) and "row selected" or "row",
        })

        local name_el = dom.create(row, "text", { class = "cmd-name" })
        for _, seg in ipairs(segments_from_matches(entry.cmd.name, entry.matched)) do
            dom.create(name_el, "span", {
                class = seg.matched and "cmd-match" or "cmd-name",
                text = seg.text,
            })
        end

        local desc = entry.cmd.description
        if desc ~= nil and desc ~= "" then
            dom.create(row, "text", { class = "cmd-desc", text = desc })
        end

        entry.el = row
    end
end

local function update_preview()
    if preview_el == nil then return end
    if selected_index ~= nil and visible[selected_index] ~= nil then
        local cmd = visible[selected_index].cmd
        dom.set_text(preview_el, "Tab: " .. cmd.name)
    else
        dom.set_text(preview_el, "")
    end
end

local function set_selected(new_idx)
    if new_idx == selected_index then
        return
    end
    selected_index = new_idx
    render_window()
    update_preview()
end

local function parse_input_line(raw)
    local trimmed = (raw or ""):match("^%s*(.-)%s*$")
    local space_pos = string.find(trimmed, "%s+")
    if space_pos ~= nil then
        local cmd = string.sub(trimmed, 1, space_pos - 1)
        local args = string.sub(trimmed, space_pos)
        return cmd, args
    end
    return trimmed, ""
end

local function rebuild_list(raw_input)
    current_raw_input = raw_input or ""
    local cmd_query, args = parse_input_line(current_raw_input)
    local scored = {}

    for _, cmd in ipairs(all_commands) do
        local s, m = fuzzy_match(cmd.name, cmd_query)
        if s ~= nil then
            table.insert(scored, { cmd = cmd, score = s, matched = m })
        end
    end

    if cmd_query ~= "" then
        table.sort(scored, function(a, b)
            if a.score ~= b.score then
                return a.score > b.score
            end
            return a.cmd.name < b.cmd.name
        end)
    end

    for i = #scored, MAX_ROWS + 1, -1 do
        scored[i] = nil
    end

    visible = scored
    selected_index = (cmd_query ~= "" and #visible > 0) and 1 or nil

    render_window()
    update_preview()
end

local function close_palette()
    state.resume()
    menus.close(current_menu_name)
end

function on_menu_ready(menu_name, root_id)
    state.pause()
    current_menu_name = menu_name
    all_commands = commands.list()
    list_el = dom.get_by_id(root_id, "palette_list")
    search_el = dom.get_by_id(root_id, "search")
    preview_el = dom.get_by_id(root_id, "suggestion_preview")

    dom.on(search_el, "text_change", "on_search_change")
    dom.on(search_el, "key_down", "on_search_key")

    rebuild_list("")
end

function on_search_change(_, text)
    rebuild_list(text)
end

function on_search_key(_, ev)
    local count = #visible
    local key = ev.key

    if key == "Escape" or (key == "Backquote" and not ev["repeat"]) then
        close_palette()
    elseif key == "ArrowDown" then
        if count > 0 then
            if selected_index == nil or selected_index >= count then
                set_selected(1)
            else
                set_selected(selected_index + 1)
            end
        end
    elseif key == "ArrowUp" then
        if count > 0 then
            if selected_index == nil or selected_index <= 1 then
                set_selected(count)
            else
                set_selected(selected_index - 1)
            end
        end
    elseif key == "Tab" then
        if selected_index ~= nil and visible[selected_index] ~= nil then
            local _, args = parse_input_line(current_raw_input)
            local cmd_name = visible[selected_index].cmd.name
            local new_val = cmd_name .. args
            if args == "" then
                new_val = cmd_name .. " "
            end
            dom.set_value(search_el, new_val)
        end
    elseif key == "Enter" then
        local full_command = current_raw_input:match("^%s*(.-)%s*$")
        if selected_index ~= nil and visible[selected_index] ~= nil then
            local _, args = parse_input_line(current_raw_input)
            local cmd_name = visible[selected_index].cmd.name
            if args ~= "" then
                full_command = cmd_name .. args
            else
                full_command = cmd_name
            end
        end

        if full_command ~= "" then
            commands.run(full_command)
            dom.set_value(search_el, "")
            rebuild_list("")
        end
    end
end