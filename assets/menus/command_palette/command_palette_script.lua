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

local MAX_ROWS = 20
local all_commands = {}
local visible_commands = {}
local spawned_row_count = 0
local selected_index = nil
local current_query = ""

local NORMAL_BG = "#252834"
local SELECTED_BG = "#3E4A6B"
local MATCHED_COLOR = "#4F92FF"
local NORMAL_COLOR = "#FFFFFF"

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
            if at_boundary then
                score = score + 25
            end
            if prev_matched then
                score = score + 15
            end
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
        return { { text = candidate, color = NORMAL_COLOR } }
    end

    local segments = {}
    local current_text = ""
    local current_matched = nil

    for i = 1, #candidate do
        local m = matched[i] == true
        local char = string.sub(candidate, i, i)
        if current_matched == nil then
            current_matched = m
            current_text = char
        elseif current_matched == m then
            current_text = current_text .. char
        else
            table.insert(segments, {
                text = current_text,
                color = current_matched and MATCHED_COLOR or NORMAL_COLOR,
            })
            current_matched = m
            current_text = char
        end
    end
    if #current_text > 0 then
        table.insert(segments, {
            text = current_text,
            color = current_matched and MATCHED_COLOR or NORMAL_COLOR,
        })
    end

    return segments
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

    visible_commands = {}
    local visible_count = math.min(#scored, spawned_row_count)
    selected_index = (current_query ~= "" and visible_count > 0) and 1 or nil

    for i = 1, spawned_row_count do
        local row_idx = i - 1
        if i <= visible_count then
            local entry = scored[i]
            table.insert(visible_commands, entry.cmd)

            local bg = (i == selected_index) and SELECTED_BG or NORMAL_BG
            local desc = entry.cmd.description
            if desc == nil or desc == "" then
                desc = " "
            end

            local rich_segments = segments_from_matches(entry.cmd.name, entry.matched)

            menus.set_property(menu_name, "palette_list", row_idx, "display", "flex")
            menus.set_property(menu_name, "palette_list", row_idx, "bg", bg)
            menus.set_child_rich_text(menu_name, "palette_list", row_idx, "name_box", rich_segments, 14.0)
            menus.set_child_text(menu_name, "palette_list", row_idx, "desc_text", desc)
        else
            menus.set_property(menu_name, "palette_list", row_idx, "display", "none")
        end
    end
end

function on_menu_ready(menu_name)
    all_commands = get_commands().list()
    spawned_row_count = math.min(#all_commands, MAX_ROWS)

    local menus = get_menus()
    menus.clear_children(menu_name, "palette_list")

    for _ = 1, spawned_row_count do
        menus.spawn_component(menu_name, "palette_list", "components/command_row.html", {
            bg = NORMAL_BG,
            display = "flex",
        })
    end

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