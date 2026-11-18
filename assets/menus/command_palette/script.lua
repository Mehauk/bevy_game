local dom      = import_namespace("dom")
local commands = import_namespace("commands")
local menus    = import_namespace("menus")

function on_dom_event(handler, node, payload)
    local fn = _G[handler]
    if fn ~= nil then
        fn(node, payload)
    end
end

local MAX_ROWS = 20
local all_commands = {}
local visible = {}
local selected_index = nil
local current_query = ""

local list_el = nil
local search_el = nil

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

local function set_selected(new_idx)
    if selected_index ~= nil and visible[selected_index] ~= nil then
        dom.remove_class(visible[selected_index].el, "selected")
    end
    selected_index = new_idx
    if selected_index ~= nil and visible[selected_index] ~= nil then
        dom.add_class(visible[selected_index].el, "selected")
    end
end

local function rebuild_list(query)
    current_query = string.lower((query or ""):match("^%s*(.-)%s*$"))
    local scored = {}

    for _, cmd in ipairs(all_commands) do
        local s, m = fuzzy_match(cmd.name, current_query)
        if s ~= nil then
            table.insert(scored, { cmd = cmd, score = s, matched = m })
        end
    end

    if current_query ~= "" then
        table.sort(scored, function(a, b)
            if a.score ~= b.score then
                return a.score > b.score
            end
            return a.cmd.name < b.cmd.name
        end)
    end

    dom.clear(list_el)
    visible = {}
    local count = math.min(#scored, MAX_ROWS)
    selected_index = (current_query ~= "" and count > 0) and 1 or nil

    for i = 1, count do
        local entry = scored[i]
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

        visible[i] = { el = row, cmd = entry.cmd }
    end
end

function on_menu_ready(_, root_id)
    all_commands = commands.list()
    list_el = dom.get_by_id(root_id, "palette_list")
    search_el = dom.get_by_id(root_id, "search")

    dom.on(dom.get_by_id(root_id, "backdrop"), "click", "on_backdrop_click")
    dom.on(search_el, "text_change", "on_search_change")
    dom.on(search_el, "key_down", "on_search_key")

    rebuild_list("")
end

function on_backdrop_click()
    menus.close_top()
end

function on_search_change(_, text)
    rebuild_list(text)
end

function on_search_key(_, ev)
    local count = #visible
    local key = ev.key

    if key == "ArrowDown" then
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
            dom.set_value(search_el, visible[selected_index].cmd.name)
        end
    elseif key == "Enter" then
        local target = nil
        if selected_index ~= nil and visible[selected_index] ~= nil then
            target = visible[selected_index].cmd.name
        elseif current_query ~= "" then
            for _, cmd in ipairs(all_commands) do
                if string.lower(cmd.name) == current_query then
                    target = cmd.name
                    break
                end
            end
        end

        if target ~= nil then
            menus.close_top()
            commands.run(target)
        elseif current_query == "" then
            menus.close_top()
        end
    end
end