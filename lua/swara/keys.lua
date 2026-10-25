-- Swara's keys, set in each LilyPond buffer when the `keys` option is on. Each belongs to its
-- buffer alone and carries a description, so a menu of keys can name it.

local M = {}

-- Each key after `<localleader>`, the `:Swara` subcommand it runs, and its description.
local KEYS = {
  { 'o', 'open', 'Swara: open the PDF' },
  { 'v', 'preview', 'Swara: preview the score in the editor' },
  { 'p', 'play', 'Swara: play the score' },
  { 's', 'stop', 'Swara: stop playing' },
  { 'm', 'musescore', 'Swara: open the score in MuseScore' },
  { 'b', 'build', 'Swara: build the PDF and MIDI' },
}

--- Sets Swara's keys in `buf`, unless the `keys` option is off.
function M.attach(buf)
  if not require('swara.config').get().keys then
    return
  end
  for _, key in ipairs(KEYS) do
    vim.keymap.set('n', '<localleader>' .. key[1], function()
      require('swara.commands').run({ fargs = { key[2] } })
    end, { buffer = buf, desc = key[3] })
  end
end

return M
