-- `:Swara`, the one command: `:Swara new guitar|piano|song <name>` writes a lesson, and the other
-- subcommands act on the current LilyPond file. The keys in `swara.keys` call these too.

local M = {}

-- In the order completion offers them.
local SUBCOMMANDS = { 'new', 'build', 'open', 'preview', 'play', 'stop', 'musescore' }

local KINDS = { 'guitar', 'piano', 'song' }

-- The subcommands that work from the file on disk or from the server, so an unnamed buffer is
-- asked to be saved first.
local SAVED = { build = true, preview = true, play = true, musescore = true }

local function warn(message)
  vim.notify(message, vim.log.levels.WARN)
end

--- Asks Swara's server to build `buf`. The server builds the text in the editor, saved or not,
--- answers at once, and reports through `swara/buildFinished` once LilyPond is done.
local function build(buf)
  local client = vim.lsp.get_clients({ bufnr = buf, name = 'swara' })[1]
  if not client then
    warn('Swara: the server is not running for this file')
    return
  end
  local params = { command = 'swara.build', arguments = { vim.uri_from_bufnr(buf) } }
  client:request('workspace/executeCommand', params, function(err)
    if err then
      warn('Swara: ' .. err.message)
    end
  end, buf)
end

local ACTIONS = {
  build = build,
  open = function(buf)
    require('swara.viewer').open(buf)
  end,
  preview = function(buf)
    require('swara.preview').show(buf)
  end,
  play = function(buf)
    require('swara.playback').play(buf)
  end,
  stop = function()
    require('swara.playback').stop()
  end,
  musescore = function(buf)
    require('swara.musescore').open(buf)
  end,
}

--- Runs `:Swara` with the arguments in `opts.fargs`, on the current buffer.
function M.run(opts)
  local sub = opts.fargs[1]
  if sub == 'new' then
    require('swara.templates').new(opts.fargs[2], table.concat(opts.fargs, ' ', 3), function(path)
      vim.cmd.edit(vim.fn.fnameescape(path))
    end)
    return
  end
  local action = ACTIONS[sub]
  if not action then
    warn(('Swara: no subcommand `%s`; use %s'):format(tostring(sub), table.concat(SUBCOMMANDS, ', ')))
    return
  end
  local buf = vim.api.nvim_get_current_buf()
  if SAVED[sub] and vim.api.nvim_buf_get_name(buf) == '' then
    warn('Swara: save the file first')
    return
  end
  action(buf)
end

--- Completes `:Swara`: the subcommands first, then the kinds of lesson after `new`.
function M.complete(arglead, cmdline, cursorpos)
  local words = vim.split(vim.trim(cmdline:sub(1, cursorpos)), '%s+')
  -- The words before the one being typed, the command's own name among them.
  local done = #words - (arglead == '' and 0 or 1)
  local choices
  if done == 1 then
    choices = SUBCOMMANDS
  elseif done == 2 and words[2] == 'new' then
    choices = KINDS
  else
    return {}
  end
  return vim.tbl_filter(function(choice)
    return vim.startswith(choice, arglead)
  end, choices)
end

return M
