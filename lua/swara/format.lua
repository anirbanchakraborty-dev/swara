-- Tidies a LilyPond buffer through Swara's server each time it is saved, unless `format_on_save`
-- is off or conform.nvim tidies the buffer itself.

local config = require('swara.config')

local M = {}

local group = vim.api.nvim_create_augroup('swara.format', { clear = true })

--- The formatters conform.nvim's `formatters_by_ft` names for LilyPond, as a set. Its entry is a
--- list of names, which may hold lists of names, or a function of the buffer that returns one.
local function lilypond_formatters(conform, buf)
  local entry = type(conform.formatters_by_ft) == 'table' and conform.formatters_by_ft.lilypond or nil
  if type(entry) == 'function' then
    local ok, got = pcall(entry, buf)
    entry = ok and got or nil
  end
  local names = {}
  local function collect(list)
    for _, item in ipairs(list) do
      if type(item) == 'string' then
        names[item] = true
      elseif type(item) == 'table' then
        collect(item)
      end
    end
  end
  if type(entry) == 'table' then
    collect(entry)
  end
  return names
end

--- Whether conform.nvim is loaded and will run a formatter it has for LilyPond on `buf`, in which
--- case it tidies on save and Swara stays out of its way. A formatter conform runs on every
--- filetype, under `*` or `_`, is not one. conform's public call for what it runs is
--- `list_formatters_to_run(bufnr)`, which returns the formatters and whether it falls back to the
--- LSP, as read in conform.nvim v9.1.0 and on its master branch on 2026-10-25.
local function conform_tidies(buf)
  local conform = package.loaded.conform
  if type(conform) ~= 'table' or type(conform.list_formatters_to_run) ~= 'function' then
    return false
  end
  local ok, formatters = pcall(conform.list_formatters_to_run, buf)
  if not ok or type(formatters) ~= 'table' then
    return false
  end
  local names = lilypond_formatters(conform, buf)
  return vim.iter(formatters):any(function(formatter)
    return type(formatter) == 'table' and names[formatter.name] == true
  end)
end

--- Tidies `buf` before each write when `format_on_save` is on. A save before the server has
--- started writes the text as it is.
function M.attach(buf)
  vim.api.nvim_clear_autocmds({ group = group, buffer = buf })
  if not config.get().format_on_save then
    return
  end
  vim.api.nvim_create_autocmd('BufWritePre', {
    group = group,
    buffer = buf,
    desc = 'Tidy the LilyPond file with swara',
    callback = function()
      if conform_tidies(buf) or not vim.lsp.get_clients({ bufnr = buf, name = 'swara' })[1] then
        return
      end
      vim.lsp.buf.format({ bufnr = buf, name = 'swara', timeout_ms = 200 })
    end,
  })
end

return M
