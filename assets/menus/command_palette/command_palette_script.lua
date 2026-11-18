local _commands = nil
local _menus = nil

local function get_commands()
    if not _commands then
        _commands = import_namespace("commands")
    end
    return _commands
end

local function get_menus()
    if not _menus then
        _menus = import_namespace("menus")
    end
    return _menus
end

local all_commands = {}
local visible_commands = {}
local pending_row_segments = {}
local selected_index = nil
local current_query = ""

local NORMAL_BG = "#252834"
local SELECTED_BG = "#3E4A6B"

local function get_fuzzy_segments(candidate, query)
    if query == "" then
        return { { text = candidate, matched = false } }
    end

    local c_lower = string.lower(candidate)
    local q_lower = string.lower(query)
    local matched = {}
    for i = 1, #candidate do
        matched[i] = false
    end

    local qi = 1
    local qlen = #q_lower
    for i = 1, #c_lower do
        if qi > qlen then
            break
        end
        if string.sub(c_lower, i, i) == string.sub(q_lower, qi, qi) then
            matched[i] = true
            qi = qi + 1
        end
    end

    local segments = {}
    local current_text = ""
    local current_matched = nil

    for i = 1, #candidate do
        local m = matched[i]
        local char = string.sub(candidate, i, i)
        if current_matched == nil then
            current_matched = m
            current_text = char
        elseif current_matched == m then
            current_text = current_text .. char
        else
            table.insert(segments, { text = current_text, matched = current_matched })
            current_matched = m
            current_text = char
        end
    end
    if #current_text > 0 then
        table.insert(segments, { text = current_text, matched = current_matched })
    end

    return segments
end

local function fuzzy_score(candidate, query)
    if query == "" then
        return 0
    end
    if candidate == query then
        return 10000
    end
    local start_pos = string.find(candidate, query, 1, true)
    if start_pos ~= nil then
        local bonus = (start_pos == 1) and 500 or 0
        return 5000 + bonus - #candidate
    end

    local qi = 1
    local qlen = #query
    local score = 0
    local prev_matched = false
    local prev_char = nil

    for i = 1, #candidate do
        if qi > qlen then
            break
        end
        local c = string.sub(candidate, i, i)
        local q = string.sub(query, qi, qi)
        if c == q then
            local at_boundary = prev_char == nil
                or prev_char == ">"
                or prev_char == "_"
                or prev_char == "-"
                or prev_char == " "
                or prev_char == ":"
            score = score + 10
            if at_boundary then
                score = score + 25
            end
            if prev_matched then
                score = score + 15
            end
            prev_matched = true
            qi = qi + 1
        else
            prev_matched = false
        end
        prev_char = c
    end

    if qi > qlen then
        return score - #candidate
    end
    return nil
end

local function update_highlight(menu_name, old_idx, new_idx)
    local menus = get_menus()
    if old_idx ~= nil then
        menus.set_property(menu_name, "palette_list", old_idx - 1, "bg", NORMAL_BG)
    end
    if new_idx ~= nil then
        menus.set_property(menu_name, "palette_list", new_idx - 1, "bg", SELECTED_BG)
    end
end

local function rebuild_list(menu_name, query)
    local menus = get_menus()
    current_query = string.lower((query or ""):match("^%s*(.-)%s*$"))
    local scored = {}

    for _, cmd in ipairs(all_commands) do
        local s = fuzzy_score(string.lower(cmd.name), current_query)
        if s ~= nil then
            table.insert(scored, { cmd = cmd, score = s })
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

    visible_commands = {}
    pending_row_segments = {}
    menus.clear_children(menu_name, "palette_list")

    selected_index = (current_query ~= "" and #scored > 0) and 1 or nil

    for i, entry in ipairs(scored) do
        if i > 20 then
            break
        end
        table.insert(visible_commands, entry.cmd)
        local bg = (i == selected_index) and SELECTED_BG or NORMAL_BG

        local desc = entry.cmd.description
        if desc == nil or desc == "" then
            desc = " "
        end

        pending_row_segments[entry.cmd.name] = get_fuzzy_segments(entry.cmd.name, current_query)

        menus.spawn_component(menu_name, "palette_list", "components/command_row.html", {
            cmd = entry.cmd.name,
            desc = desc,
            bg = bg,
        })
    end
end

function on_menu_ready(menu_name)
    all_commands = get_commands().list()
    rebuild_list(menu_name, "")
end

function on_menu_text_changed(menu_name, text)
    rebuild_list(menu_name, text)
end

function on_menu_key(menu_name, key)
    local count = #visible_commands
    local commands = get_commands()
    local menus = get_menus()

    if key == "ArrowDown" then
        if count > 0 then
            local old = selected_index
            if selected_index == nil or selected_index >= count then
                selected_index = 1
            else
                selected_index = selected_index + 1
            end
            update_highlight(menu_name, old, selected_index)
        end
    elseif key == "ArrowUp" then
        if count > 0 then
            local old = selected_index
            if selected_index == nil or selected_index <= 1 then
                selected_index = count
            else
                selected_index = selected_index - 1
            end
            update_highlight(menu_name, old, selected_index)
        end
    elseif key == "Tab" then
        if selected_index ~= nil and visible_commands[selected_index] ~= nil then
            menus.set_input_text(menu_name, visible_commands[selected_index].name)
        end
    elseif key == "Enter" then
        local target = nil
        if selected_index ~= nil and visible_commands[selected_index] ~= nil then
            target = visible_commands[selected_index].name
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

function on_menu_action(menu_name, tags)
    if tags.target ~= nil and tags.cmd ~= nil then
        local segments = pending_row_segments[tags.cmd]
        if segments ~= nil then
            local menus = get_menus()
            for _, seg in ipairs(segments) do
                if seg.text ~= "" then
                    menus.spawn_component(menu_name, tags.target, "components/text_span.html", {
                        text = seg.text,
                        color = seg.matched and "#4F92FF" or "#FFFFFF",
                    })
                end
            end
            pending_row_segments[tags.cmd] = nil
        end
    elseif tags.callback == "on_row_pressed" and tags.cmd ~= nil then
        get_commands().run(tags.cmd)
        get_menus().close_top()
    end
end

