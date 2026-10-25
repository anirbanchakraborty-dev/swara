local h = require('helpers')

local T = {}

function T.defaults_without_setup()
  local got = h.child_json([[
    vim.cmd.runtime('plugin/swara.lua')
    vim.bo.filetype = 'lilypond'
    local autocmds = vim.api.nvim_get_autocmds({ group = 'swara', event = 'FileType', pattern = 'lilypond' })
    print(vim.json.encode({ config = require('swara.config').get(), autocmds = #autocmds }))
  ]])
  assert(got.autocmds == 1, 'FileType autocommands: ' .. got.autocmds)
  local c = got.config
  assert(c.auto_check == true and c.format_on_save == true and c.keys == true, vim.inspect(c))
  assert(c.debounce_ms == 10 and c.preview_dpi == 120, vim.inspect(c))
  assert(c.soundfont == nil and c.lilypond == nil, vim.inspect(c))
  assert(vim.tbl_isempty(c.include_paths), vim.inspect(c))
  assert(c.release_base_url == 'https://github.com/anirbanchakraborty-dev/swara/releases/download', c.release_base_url)
end

function T.setup_merges()
  local c = h.child_json([[
    require('swara').setup({ debounce_ms = 50, include_paths = { '/music/ly' }, soundfont = '/sf/a.sf2' })
    print(vim.json.encode(require('swara.config').get()))
  ]])
  assert(c.debounce_ms == 50, vim.inspect(c))
  assert(c.soundfont == '/sf/a.sf2', vim.inspect(c))
  assert(#c.include_paths == 1 and c.include_paths[1] == '/music/ly', vim.inspect(c))
  assert(c.auto_check == true and c.preview_dpi == 120, vim.inspect(c))
end

function T.unknown_key_warns_once()
  local got = h.child_json([[
    local warnings = {}
    vim.notify = function(message, level) table.insert(warnings, { message, level }) end
    require('swara').setup({ colour = 'red', debounce_ms = 20 })
    require('swara').setup({ colour = 'red', debounce_ms = 20 })
    print(vim.json.encode({ warnings = warnings, config = require('swara.config').get() }))
  ]])
  assert(#got.warnings == 1, vim.inspect(got.warnings))
  local message, level = got.warnings[1][1], got.warnings[1][2]
  assert(message:find('colour', 1, true), message)
  assert(level == vim.log.levels.WARN, tostring(level))
  assert(got.config.colour == nil, 'unknown key was kept')
  assert(got.config.debounce_ms == 20, 'known key beside it was dropped')
end

function T.version_matches_cargo()
  local cargo = assert(h.read(h.root .. '/crates/swara/Cargo.toml'))
  local package = cargo:match('%[package%](.-)\n%[') or cargo
  local version = package:match('\nversion%s*=%s*"([^"]+)"')
  assert(version, 'no version in crates/swara/Cargo.toml')
  local lua = require('swara.version')
  assert(lua == version, ('version.lua says %s, Cargo.toml says %s'):format(lua, version))
end

return T
