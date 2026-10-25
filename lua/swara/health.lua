-- `:checkhealth swara`: one line for each thing Swara uses, with the fix for each that is missing
-- or off. It only reads. The Skim preferences it checks are shown as commands to run, never set.

local M = {}

local health = vim.health

-- The two commands that turn on Skim's own reload, in the order of `swara.viewer`'s WATCH.
local SKIM_ADVICE = {
  'Close Skim, then run these two commands in a terminal:',
  'defaults write -app Skim SKAutoCheckFileUpdate -bool true',
  'defaults write -app Skim SKAutoReloadFileUpdate -bool true',
}

--- The `swara` program, found the way the plugin finds it, waiting up to a minute for a download.
--- Returns the program, or nil and why there is none.
local function program()
  local done, found, why = false, nil, nil
  require('swara.binary').resolve(function(bin, message)
    done, found, why = true, bin, message
  end)
  if not vim.wait(60000, function()
    return done
  end, 50) then
    return nil, 'the search for it did not end within a minute'
  end
  return found, why
end

local function report_program(bin)
  local info = bin.info
  local want = require('swara.version')
  if info.version == want then
    health.ok(('swara %s, the version this plugin expects'):format(info.version))
  else
    health.warn(
      ('swara %s, but this plugin expects %s'):format(tostring(info.version), want),
      { 'Update the plugin, which downloads the matching swara, or install swara ' .. want }
    )
  end
  if bin.source == 'download' then
    health.info('swara was downloaded by the plugin: ' .. bin.path)
  else
    health.info('swara was found on PATH: ' .. bin.path)
  end
end

local function report_grammar(info)
  local ok, why = require('swara.treesitter').check(info)
  if ok then
    health.ok(('LilyPond grammar from commit %s'):format(tostring(info.grammar_rev)))
    return
  end
  health.error(('LilyPond grammar from commit %s: %s'):format(tostring(info.grammar_rev), why), {
    "Swara's colours, folds and indent need its own grammar under the name `lilypond`.",
    'Remove the other LilyPond parser from the runtime path, then restart Neovim.',
  })
end

local function report_lilypond(info)
  local lilypond = info.lilypond
  if type(lilypond) == 'table' then
    health.ok(('LilyPond %s: %s'):format(tostring(lilypond.version), tostring(lilypond.path)))
    return
  end
  health.warn('LilyPond not found, so the check, builds, preview and playback are off', {
    "Install LilyPond (`brew install lilypond` on macOS), or set `lilypond` in Swara's options to its path.",
  })
end

local function report_cache(info)
  local cache = type(info.cache) == 'table' and info.cache or {}
  if cache.present == true then
    health.ok('command cache: ' .. tostring(cache.path))
  else
    health.info(
      'no command cache yet: the server makes it from LilyPond the first time it needs it, at ' .. tostring(cache.path)
    )
  end
end

local function report_playback()
  if vim.fn.executable('fluidsynth') == 1 then
    health.ok('FluidSynth: ' .. vim.fn.exepath('fluidsynth'))
  else
    health.warn('FluidSynth not found, so playback is off', { 'Install it (`brew install fluid-synth` on macOS).' })
  end
  local font, why = require('swara.playback').soundfont()
  if font then
    health.ok('soundfont: ' .. font)
  else
    health.warn('no soundfont to play with, so playback is off', { (why:gsub('^Swara: ', '')) })
  end
end

local function report_musescore()
  local path = require('swara.musescore').find()
  if path then
    health.ok('MuseScore: ' .. path)
  else
    health.warn('MuseScore not found, so `:Swara musescore` is off', {
      'Install MuseScore 4 (`brew install --cask musescore` on macOS).',
    })
  end
end

--- Whether the Skim preference `key` reads 1. An unset preference makes `defaults` exit 1 and
--- means Skim's shipped value, which is off.
local function skim_reads_on(key)
  local result = vim.system({ 'defaults', 'read', '-app', 'Skim', key }, { text = true }):wait(5000)
  return result.code == 0 and vim.trim(result.stdout or '') == '1'
end

local function report_skim()
  local viewer = require('swara.viewer')
  if not viewer.has_skim() then
    health.info('Skim is not installed, so a PDF opens in the default viewer, which may not reload it after a build')
    return
  end
  local off = {}
  for _, key in ipairs(viewer.WATCH) do
    if not skim_reads_on(key) then
      table.insert(off, key)
    end
  end
  if #off == 0 then
    health.ok('Skim reloads a PDF by itself when a build changes it')
  else
    health.warn(
      ('Skim does not reload a PDF by itself (%s off), so Swara asks it to after each build'):format(
        table.concat(off, ' and ')
      ),
      SKIM_ADVICE
    )
  end
end

function M.check()
  health.start('swara')
  local bin, why = program()
  if not bin then
    health.error('swara program not found: ' .. tostring(why), {
      'Put `swara` on PATH, or check that the plugin can reach its release downloads.',
    })
    return
  end
  report_program(bin)
  report_grammar(bin.info)
  report_lilypond(bin.info)
  report_cache(bin.info)
  report_playback()
  report_musescore()
  if vim.fn.has('mac') == 1 then
    report_skim()
  end
end

return M
