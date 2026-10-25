local h = require('helpers')

local M = {}

-- Child code that replaces the search for the `swara` program with one that never answers, so a
-- LilyPond buffer gets its keys and nothing else.
local QUIET_BINARY = [[
  package.loaded['swara.binary'] = { resolve = function() end }
]]

-- Child code that records each `vim.health` report as { kind, message } in `report`, and answers
-- the search for the `swara` program with `info`.
local HEALTH_PRELUDE = [[
  local h = require('helpers')
  local report = {}
  for _, kind in ipairs({ 'start', 'ok', 'info', 'warn', 'error' }) do
    vim.health[kind] = function(message, advice)
      table.insert(report, { kind = kind, message = message, advice = advice })
    end
  end
  local info = h.info()
  package.loaded['swara.binary'] = {
    resolve = function(callback)
      callback({ path = 'swara', source = 'path', info = info })
    end,
  }
]]

--- The reports of `kind` in `report`.
local function only(report, kind)
  return vim.tbl_filter(function(line)
    return line.kind == kind
  end, report)
end

function M.completes_subcommands()
  local out = h.child_json([[
    vim.cmd.runtime('plugin/swara.lua')
    print(vim.json.encode({
      sub = vim.fn.getcompletion('Swara ', 'cmdline'),
      kinds = vim.fn.getcompletion('Swara new ', 'cmdline'),
      after = vim.fn.getcompletion('Swara build ', 'cmdline'),
    }))
  ]])
  assert(
    vim.deep_equal(out.sub, { 'new', 'build', 'open', 'preview', 'play', 'stop', 'musescore' }),
    vim.inspect(out.sub)
  )
  assert(vim.deep_equal(out.kinds, { 'guitar', 'piano', 'song' }), vim.inspect(out.kinds))
  assert(#out.after == 0, vim.inspect(out.after))
end

function M.keys_only_in_lilypond()
  local script = QUIET_BINARY
    .. [[
    vim.g.maplocalleader = ','
    %s
    vim.cmd.runtime('plugin/swara.lua')
    --- The keys of a new buffer of `filetype`, as { lhs = desc }.
    local function keys(filetype)
      local buf = vim.api.nvim_create_buf(true, false)
      vim.api.nvim_set_current_buf(buf)
      vim.bo[buf].filetype = filetype
      local found = {}
      for _, map in ipairs(vim.api.nvim_buf_get_keymap(buf, 'n')) do
        found[map.lhs] = map.desc or ''
      end
      return found
    end
    print(vim.json.encode({ lilypond = keys('lilypond'), markdown = keys('markdown') }))
  ]]
  local on = h.child_json(script:format(''))
  for _, key in ipairs({ 'o', 'v', 'p', 's', 'm', 'b' }) do
    local desc = on.lilypond[',' .. key]
    assert(type(desc) == 'string' and desc ~= '', ',' .. key .. ' missing or without a desc: ' .. vim.inspect(on))
  end
  local off = h.child_json(script:format("require('swara').setup({ keys = false })"))
  -- Neovim's own markdown plugin sets keys of its own, so only Swara's are looked for.
  for _, key in ipairs({ 'o', 'v', 'p', 's', 'm', 'b' }) do
    assert(on.markdown[',' .. key] == nil, ',' .. key .. ' in a markdown buffer: ' .. vim.inspect(on.markdown))
    assert(off.lilypond[',' .. key] == nil, ',' .. key .. ' with keys = false: ' .. vim.inspect(off.lilypond))
  end
end

function M.unnamed_buffer_says_save_first()
  local out = h.child_json(QUIET_BINARY .. [[
    local messages = {}
    vim.notify = function(message)
      table.insert(messages, message)
    end
    vim.cmd.runtime('plugin/swara.lua')
    vim.bo.filetype = 'lilypond'
    local said = {}
    for _, sub in ipairs({ 'build', 'preview', 'play', 'musescore' }) do
      messages = {}
      vim.cmd('Swara ' .. sub)
      said[sub] = messages
    end
    print(vim.json.encode(said))
  ]])
  for _, sub in ipairs({ 'build', 'preview', 'play', 'musescore' }) do
    local messages = out[sub]
    assert(
      #messages == 1 and messages[1]:lower():find('save the file first', 1, true),
      sub .. ': ' .. vim.inspect(messages)
    )
  end
end

function M.health_without_lilypond()
  local out = h.child_json(HEALTH_PRELUDE .. [[
    info.lilypond = vim.NIL
    h.fake_bin('fluidsynth', 'exit 0')
    require('swara.musescore').find = function()
      return '/Applications/MuseScore 4.app'
    end
    require('swara').setup({ soundfont = h.write(h.tmpdir() .. '/basic.sf3', 'not a real soundfont') })
    local system = vim.system
    vim.system = function(cmd, opts, on_exit)
      if cmd[1] == 'defaults' then
        local result = { code = 0, signal = 0, stdout = '1\n', stderr = '' }
        if on_exit then
          on_exit(result)
        end
        return { wait = function() return result end }
      end
      return system(cmd, opts, on_exit)
    end
    require('swara.health').check()
    h.cleanup()
    print(vim.json.encode(report))
  ]])
  local warnings = only(out, 'warn')
  assert(#warnings == 1, 'expected one warning, got ' .. vim.inspect(warnings))
  for _, word in ipairs({ 'check', 'build', 'preview', 'playback' }) do
    assert(warnings[1].message:find(word, 1, true), warnings[1].message)
  end
  assert(#only(out, 'error') == 0, vim.inspect(only(out, 'error')))
  local grammar = vim.tbl_filter(function(line)
    return line.message:find('grammar', 1, true) ~= nil
  end, out)
  assert(#grammar >= 1 and grammar[1].kind == 'ok', vim.inspect(grammar))
end

function M.health_reports_conflict()
  local out = h.child_json(HEALTH_PRELUDE .. [[
    local c = vim.api.nvim_get_runtime_file('parser/c.*', false)[1]
    vim.treesitter.language.add('lilypond', { path = c, symbol_name = 'c' })
    require('swara.health').check()
    print(vim.json.encode(report))
  ]])
  local conflict = vim.tbl_filter(function(line)
    return line.message:find('another LilyPond grammar', 1, true) ~= nil
  end, out)
  assert(#conflict == 1 and conflict[1].kind == 'error', vim.inspect(out))
  for _, line in ipairs(out) do
    assert(not (line.kind == 'ok' and line.message:find('grammar', 1, true)), 'grammar reported OK: ' .. line.message)
  end
end

function M.health_names_other_queries()
  -- The grammar is Swara's own here, but another plugin's highlights come first on the runtime
  -- path, which is what stops the colours, so the report names that file.
  local dir = h.tmpdir()
  local other = h.write(dir .. '/queries/lilypond/highlights.scm', '(no_such_node) @variable\n')
  local out = h.child_json(HEALTH_PRELUDE .. ([[
    vim.opt.runtimepath:prepend(%q)
    require('swara.health').check()
    print(vim.json.encode(report))
  ]]):format(dir))
  local named = vim.tbl_filter(function(line)
    return line.kind == 'error' and line.message:find(other, 1, true) ~= nil
  end, out)
  assert(#named == 1, vim.inspect(out))
  for _, line in ipairs(out) do
    assert(not (line.kind == 'ok' and line.message:find('grammar', 1, true)), 'grammar reported OK: ' .. line.message)
  end
end

function M.health_is_found_by_checkhealth()
  -- `:checkhealth` prints its progress after anything the child prints, so the report is looked
  -- for in the whole output rather than decoded from the last line.
  local out = h.child([[
    local info = require('helpers').info()
    package.loaded['swara.binary'] = {
      resolve = function(callback)
        callback({ path = 'swara', source = 'path', info = info })
      end,
    }
    vim.cmd('checkhealth swara')
    io.stdout:write(table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), '\n'), '\n')
  ]])
  assert(out.code == 0, out.stderr)
  assert(out.stdout:find('OK swara ', 1, true) and out.stdout:find('OK LilyPond grammar', 1, true), out.stdout)
end

function M.health_never_writes_defaults()
  local out = h.child_json(HEALTH_PRELUDE .. [[
    local seen = {}
    local system = vim.system
    vim.system = function(cmd, opts, on_exit)
      table.insert(seen, table.concat(cmd, ' '))
      if cmd[1] == 'defaults' then
        local result = { code = 0, signal = 0, stdout = '0\n', stderr = '' }
        if on_exit then
          on_exit(result)
        end
        return { wait = function() return result end }
      end
      return system(cmd, opts, on_exit)
    end
    require('swara.health').check()
    local skim = vim.fn.has('mac') == 1
      and (vim.fn.isdirectory('/Applications/Skim.app') == 1 or vim.fn.isdirectory(vim.fs.normalize('~/Applications/Skim.app')) == 1)
    print(vim.json.encode({ seen = seen, report = report, skim = skim }))
  ]])
  for _, cmd in ipairs(out.seen) do
    assert(not cmd:find('write', 1, true), 'ran ' .. cmd)
  end
  if out.skim then
    local reads = vim.tbl_filter(function(cmd)
      return cmd:find('^defaults read %-app Skim ') ~= nil
    end, out.seen)
    assert(#reads == 2, vim.inspect(out.seen))
    local shown = vim.json.encode(out.report)
    for _, key in ipairs({ 'SKAutoCheckFileUpdate', 'SKAutoReloadFileUpdate' }) do
      assert(shown:find('defaults write -app Skim ' .. key .. ' -bool true', 1, true), key .. ': ' .. shown)
    end
  end
end

return M
