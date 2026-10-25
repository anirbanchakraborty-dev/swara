-- Swara for Neovim. `setup` is optional: `plugin/swara.lua` attaches every LilyPond buffer with
-- the defaults when it was never called.

local M = {}

--- Sets Swara's options. See `swara.config` for the names and their defaults.
function M.setup(opts)
  require('swara.config').setup(opts)
end

--- Called for each buffer whose filetype becomes `lilypond`.
function M.attach(buf)
  local _ = buf
end

return M
