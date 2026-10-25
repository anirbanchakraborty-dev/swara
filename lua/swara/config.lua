-- Swara's settings: the defaults, merged with whatever `setup` was given.

local M = {}

local defaults = {
  auto_check = true,
  format_on_save = true,
  keys = true,
  soundfont = nil,
  debounce_ms = 10,
  preview_dpi = 120,
  lilypond = nil,
  include_paths = {},
  release_base_url = 'https://github.com/anirbanchakraborty-dev/swara/releases/download',
}

-- A key whose default is nil is absent from `defaults`, so the known names are listed apart.
local known = {
  auto_check = true,
  format_on_save = true,
  keys = true,
  soundfont = true,
  debounce_ms = true,
  preview_dpi = true,
  lilypond = true,
  include_paths = true,
  release_base_url = true,
}

local current = vim.deepcopy(defaults)
local warned = {}

--- Merges `opts` over the defaults. An unknown key is dropped, with one warning per session.
function M.setup(opts)
  current = vim.deepcopy(defaults)
  for key, value in pairs(opts or {}) do
    if known[key] then
      current[key] = value
    elseif not warned[key] then
      warned[key] = true
      vim.notify(('Swara: unknown option `%s`'):format(key), vim.log.levels.WARN)
    end
  end
end

--- The settings in force: the defaults when `setup` was never called.
function M.get()
  return current
end

return M
