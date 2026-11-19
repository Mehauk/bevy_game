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

local MAX_ROWS = 100        -- cap on filtered entries kept
local SUGGESTION_ROWS = 5   -- rows that fit in .suggestions; keep in sync with style.css
local HISTORY_ROWS = 6      -- lines that fit in .console-history; keep in sync with style.css
local WHEEL_LINES = 3       -- history lines scrolled per wheel notch
local current_menu_name = nil
local all_commands = {}
local visible = {}
local selected_index = nil
local current_raw_input = ""

local history = {}          -- executed command lines, oldest first (seeded from commands.history())
local history_scroll = 0    -- lines scrolled up from the newest entry (0 = pinned to the bottom)

local history_el = nil
local suggestions_el = nil
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

-- Renders only the window of suggestion rows that fits in the floating popup. The
-- window is positioned so the selected row sits at the bottom of the popup whenever
-- enough rows remain above it to scroll (Skyrim-console style).
local function render_window()
    dom.clear(suggestions_el)

    local count = #visible
    local first = 1
    if selected_index ~= nil then
        local max_first = math.max(count - SUGGESTION_ROWS + 1, 1)
        first = math.min(math.max(selected_index - SUGGESTION_ROWS + 1, 1), max_first)
    end
    local last = math.min(count, first + SUGGESTION_ROWS - 1)

    for i = first, last do
        local entry = visible[i]
        local row = dom.create(suggestions_el, "node", {
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

-- The history window is anchored at the newest end: `history_scroll` lines are
-- scrolled away at the bottom, so 0 shows the most recent lines hugging the input.
local function max_history_scroll()
    return math.max(#history - HISTORY_ROWS, 0)
end

local function render_history()
    dom.clear(history_el)

    local last = #history - history_scroll
    local first = math.max(1, last - HISTORY_ROWS + 1)
    for i = first, last do
        dom.create(history_el, "text", { class = "history-line", text = "> " .. history[i] })
    end
end

function on_history_wheel(_, payload)
    local delta = payload.delta_y or 0
    if delta == 0 then
        return
    end
    -- Wheel up (positive delta) reveals older lines; wheel down returns to the newest.
    local target = history_scroll + (delta > 0 and WHEEL_LINES or -WHEEL_LINES)
    history_scroll = math.min(math.max(target, 0), max_history_scroll())
    render_history()
end

local function set_suggestions_visible(show)
    if show then
        dom.add_class(suggestions_el, "visible")
    else
        dom.remove_class(suggestions_el, "visible")
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

    -- The autosuggest is a floating popup: it only exists while there is something
    -- to suggest, so the history stays visible whenever the prompt is empty.
    set_suggestions_visible(selected_index ~= nil)

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
    history_el = dom.get_by_id(root_id, "console_history")
    suggestions_el = dom.get_by_id(root_id, "suggestions")
    search_el = dom.get_by_id(root_id, "search")
    preview_el = dom.get_by_id(root_id, "suggestion_preview")

    dom.on(search_el, "text_change", "on_search_change")
    dom.on(search_el, "key_down", "on_search_key")
    dom.on(history_el, "wheel", "on_history_wheel")

    -- The script is recreated on every open, so the history lives engine-side.
    history = {}
    for _, line in ipairs(commands.history()) do
        table.insert(history, line)
    end
    history_scroll = 0
    render_history()

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
        -- Arrows only drive the suggestion popup; with an empty prompt there is
        -- nothing selected and the popup is hidden, so they do nothing.
        if selected_index ~= nil then
            if selected_index >= count then
                set_selected(1)
            else
                set_selected(selected_index + 1)
            end
        end
    elseif key == "ArrowUp" then
        if selected_index ~= nil then
            if selected_index <= 1 then
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
            table.insert(history, full_command)
            history_scroll = 0
            render_history()
            dom.set_value(search_el, "")
            rebuild_list("")
        end
    end
end
