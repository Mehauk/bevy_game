local dom       = import_namespace("dom")
local commands  = import_namespace("commands")
local menus     = import_namespace("menus")
local state     = import_namespace("state")
local clipboard = import_namespace("clipboard")

function on_dom_event(handler, node, payload)
    local fn = _G[handler]
    if fn ~= nil then
        fn(node, payload)
    end
end

local MAX_ROWS = 100        -- cap on filtered entries kept
local SUGGESTION_ROWS = 5   -- rows that fit in .suggestions; keep in sync with style.css
local HISTORY_ROWS = 6      -- entries that fit in .console-history; keep in sync with style.css
local WHEEL_LINES = 3       -- history entries scrolled per wheel notch
local current_menu_name = nil
local all_commands = {}
local visible = {}
local selected_index = nil
local current_raw_input = ""

-- Entries are { command, output } maps from commands.history(). The engine owns
-- them; this is a cache refreshed on open and on every on_history_changed.
local history = {}
local history_scroll = 0    -- entries scrolled up from the newest (0 = pinned to the bottom)
local copy_targets = {}     -- copy-button node id -> text to copy

local recall_step = 0       -- 0 = not recalling; >0 = how many entries up from the newest
local recall_set = false    -- next text_change came from a recall, not from typing

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

-- The history window is anchored at the newest end: `history_scroll` entries are
-- scrolled away at the bottom, so 0 shows the most recent entries hugging the input.
-- An entry with output renders as two lines (command + output row with a copy
-- button), so the window counts entries, not lines.
local function max_history_scroll()
    return math.max(#history - HISTORY_ROWS, 0)
end

local function render_history()
    dom.clear(history_el)
    copy_targets = {}

    local last = #history - history_scroll
    local first = math.max(1, last - HISTORY_ROWS + 1)
    for i = first, last do
        local entry = history[i]
        dom.create(history_el, "text", { class = "history-line", text = "> " .. entry.command })
        if entry.output ~= nil and entry.output ~= "" then
            local out_row = dom.create(history_el, "node", { class = "history-out-row" })
            dom.create(out_row, "text", { class = "history-output", text = entry.output })
            -- `button` renders no text of its own (only text/span do), so the
            -- label is a child; the click handler sits on the button itself.
            local btn = dom.create(out_row, "button", { class = "copy-btn" })
            dom.create(btn, "text", { class = "copy-label", text = "copy" })
            copy_targets[btn] = entry.command .. "\n" .. entry.output
            dom.on(btn, "click", "on_copy_click")
        end
    end
end

function on_copy_click(node)
    local text = copy_targets[node]
    if text ~= nil then
        clipboard.set(text)
    end
end

function on_history_changed()
    local fresh = commands.history()
    if #fresh > #history then
        history_scroll = 0
    end
    history = fresh
    render_history()
end

function on_history_wheel(_, payload)
    local delta = payload.delta_y or 0
    if delta == 0 then
        return
    end
    -- Wheel up (positive delta) reveals older entries; wheel down returns to the newest.
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

-- Shell-style recall: dir 1 walks toward older commands, -1 back toward the newest.
-- Past the newest end the input is cleared. The engine echoes the value back through
-- text_change, and `recall_set` keeps that echo from cancelling the recall.
local function recall(dir)
    local count = #history
    if count == 0 then
        return
    end
    recall_step = math.min(math.max(recall_step + dir, 0), count)
    local text = ""
    if recall_step > 0 then
        text = history[count - recall_step + 1].command
    end
    recall_set = true
    dom.set_value(search_el, text)
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

    history = commands.history()
    history_scroll = 0
    render_history()

    rebuild_list("")
end

function on_search_change(_, text)
    if recall_set then
        recall_set = false
    else
        recall_step = 0
    end
    rebuild_list(text)
end

function on_search_key(_, ev)
    local count = #visible
    local key = ev.key

    if key == "Escape" or (key == "Backquote" and not ev["repeat"]) then
        close_palette()
    elseif key == "KeyV" and ev.ctrl and not ev["repeat"] then
        local pasted = clipboard.get()
        if pasted ~= nil and pasted ~= "" then
            -- The input is single-line, so fold any newlines into spaces.
            pasted = pasted:gsub("[\r\n]+", " ")
            dom.set_value(search_el, current_raw_input .. pasted)
        end
    elseif key == "ArrowDown" then
        -- With an empty prompt (or mid-recall) the arrows walk the command history;
        -- while the suggestion popup is up they drive the selection instead.
        if recall_step > 0 or selected_index == nil then
            recall(-1)
        elseif selected_index >= count then
            set_selected(1)
        else
            set_selected(selected_index + 1)
        end
    elseif key == "ArrowUp" then
        if recall_step > 0 or selected_index == nil then
            recall(1)
        elseif selected_index <= 1 then
            set_selected(count)
        else
            set_selected(selected_index - 1)
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
            -- The engine records the entry (with its output, once the queued run
            -- finishes) and calls on_history_changed to refresh the view.
            commands.run(full_command)
            recall_step = 0
            dom.set_value(search_el, "")
            rebuild_list("")
        end
    end
end
