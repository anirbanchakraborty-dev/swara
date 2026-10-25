-- Loads Swara's grammar into Neovim and starts its colours, folds and indent in LilyPond buffers.
-- The queries are this repository's `queries/lilypond/` files, which match the grammar the
-- `swara` program ships, so they are started only on that grammar.

local M = {}

-- nil before the first attach. Then true when the language `lilypond` is Swara's grammar, or the
-- message that said why it is not, which is shown once.
local state

--- The first line of a Lua error, without the `file.lua:12: ` places in front of it.
local function reason(err)
  local line = tostring(err):match('[^\n]*')
  return line:match('.*%.lua:%d+: (.*)') or line
end

local function refuse(message)
  state = message
  vim.notify(message, vim.log.levels.WARN)
end

--- Loads the grammar `info` names as the language `lilypond`. Returns true when that language is
--- Swara's grammar, and otherwise false with why it is not. `:checkhealth swara` asks this too.
function M.check(info)
  -- `add` keeps a language already registered under the name, such as another plugin's grammar.
  local ok, added, err = pcall(vim.treesitter.language.add, 'lilypond', {
    path = info.grammar,
    symbol_name = info.grammar_symbol,
  })
  if not ok or not added then
    return false, ('could not load its LilyPond grammar (%s)'):format(reason(ok and err or added))
  end
  local symbols = vim.treesitter.language.inspect('lilypond').symbols
  if symbols.embedded_lilypond ~= true then
    return false, 'another LilyPond grammar is already loaded'
  end
  return true
end

--- Loads the grammar `info` names, once, and says whether the language `lilypond` is Swara's.
local function load(info)
  if state ~= nil then
    return state == true
  end
  local ok, why = M.check(info)
  if not ok then
    local after = why:find('^another') and ', so Swara leaves colours off' or ''
    refuse(('Swara: %s%s; see :checkhealth swara'):format(why, after))
    return false
  end
  state = true
  return true
end

--- Starts Swara's colours in `buf`, and sets its indent and folds, when `info.grammar` loads as
--- the language `lilypond`.
function M.attach(buf, info)
  if not load(info) then
    return
  end
  -- Neovim reads only the first `highlights.scm` for a language on the runtime path, so another
  -- plugin's LilyPond queries earlier on it make this fail.
  local ok, err = pcall(vim.treesitter.start, buf, 'lilypond')
  if not ok then
    refuse(('Swara: could not start its colours (%s); see :checkhealth swara'):format(reason(err)))
    return
  end
  vim.bo[buf].indentexpr = "v:lua.require'swara.indent'.get(v:lnum)"
  for _, win in ipairs(vim.fn.win_findbuf(buf)) do
    vim.wo[win][0].foldexpr = 'v:lua.vim.treesitter.foldexpr()'
  end
end

return M
