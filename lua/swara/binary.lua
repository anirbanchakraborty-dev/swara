-- Finds the `swara` program: on `PATH` first, then the copy downloaded into Neovim's data folder,
-- then a fresh download of the release that matches this plugin's version. An archive is unpacked
-- only after its SHA-256 matches the release's `SHA256SUMS`, and a failed download is not tried
-- again until Neovim starts again.

local config = require('swara.config')
local version = require('swara.version')

local M = {}

local windows = vim.fn.has('win32') == 1
local EXE = windows and 'swara.exe' or 'swara'

local TRIPLES = {
  Darwin = { arm64 = 'aarch64-apple-darwin', x86_64 = 'x86_64-apple-darwin' },
  Linux = {
    x86_64 = 'x86_64-unknown-linux-gnu',
    aarch64 = 'aarch64-unknown-linux-gnu',
    arm64 = 'aarch64-unknown-linux-gnu',
  },
  Windows_NT = { x86_64 = 'x86_64-pc-windows-msvc', AMD64 = 'x86_64-pc-windows-msvc' },
}

local found -- the result, once a lookup has succeeded
local failed -- the message, once a lookup has failed
local waiting -- callbacks for the lookup that is running

--- The release triple for this machine, such as `aarch64-apple-darwin`, or nil when no release
--- archive is built for it.
function M.triple()
  local uname = vim.uv.os_uname()
  return (TRIPLES[uname.sysname] or {})[uname.machine]
end

local function folder()
  return vim.fs.joinpath(vim.fn.stdpath('data'), 'swara')
end

--- Runs `cmd` and calls back on the main loop with its result. A program that cannot be started
--- gives exit code -1 rather than an error.
local function run(cmd, callback)
  local ok = pcall(vim.system, cmd, { text = true }, vim.schedule_wrap(callback))
  if not ok then
    vim.schedule(function()
      callback({ code = -1, stdout = '', stderr = cmd[1] .. ' could not be run' })
    end)
  end
end

--- The decoded output of `<path> info`, or nil and the reason.
local function info(path, callback)
  run({ path, 'info' }, function(out)
    if out.code ~= 0 then
      return callback(nil, ('`swara info` exited %d'):format(out.code))
    end
    local ok, decoded = pcall(vim.json.decode, out.stdout)
    if not ok or type(decoded) ~= 'table' then
      return callback(nil, '`swara info` printed no JSON')
    end
    callback(decoded)
  end)
end

--- The first line `curl` wrote, without its `curl: (6) ` prefix.
local function curl_reason(out)
  local line = vim.split(vim.trim(out.stderr or ''), '\n')[1] or ''
  line = line:gsub('^curl: %(%d+%) ', '')
  return line ~= '' and line or ('curl exited %d'):format(out.code)
end

--- Downloads `url` to `dest`. curl alone waits for ever on a server that stops sending, so it gives
--- up after 15 s without a connection, or 30 s in a row under 1 KB/s.
local function fetch(url, dest, callback)
  run({
    'curl',
    '-fsSL',
    '--connect-timeout',
    '15',
    '--speed-limit',
    '1024',
    '--speed-time',
    '30',
    '-o',
    dest,
    url,
  }, callback)
end

--- The checksum `SHA256SUMS` gives for `name`, or nil.
local function listed_sum(sums, name)
  for _, line in ipairs(vim.fn.readfile(sums)) do
    local sum, file = line:match('^(%x+)%s+%*?(.-)%s*$')
    if file == name then
      return sum:lower()
    end
  end
end

local function download(callback)
  local triple = M.triple()
  if not triple then
    local uname = vim.uv.os_uname()
    return callback(nil, ('no release for %s %s'):format(uname.sysname, uname.machine))
  end
  local archive_name = ('swara-%s-%s.%s'):format(version, triple, windows and 'zip' or 'tar.gz')
  local base = ('%s/v%s/'):format(config.get().release_base_url, version)
  local partial = vim.fs.joinpath(folder(), version .. '.partial')
  local final = vim.fs.joinpath(folder(), version)
  local sums = vim.fs.joinpath(partial, 'SHA256SUMS')
  local archive = vim.fs.joinpath(partial, archive_name)

  local function fail(reason)
    vim.fn.delete(partial, 'rf')
    callback(nil, reason)
  end

  vim.fn.delete(partial, 'rf')
  vim.fn.mkdir(partial, 'p')
  vim.notify(('Swara: downloading swara %s for %s'):format(version, triple), vim.log.levels.INFO)
  fetch(base .. 'SHA256SUMS', sums, function(got_sums)
    if got_sums.code ~= 0 then
      return fail(curl_reason(got_sums))
    end
    fetch(base .. archive_name, archive, function(got_archive)
      if got_archive.code ~= 0 then
        return fail(curl_reason(got_archive))
      end
      local want = listed_sum(sums, archive_name)
      if not want then
        return fail('no checksum for ' .. archive_name .. ' in SHA256SUMS')
      end
      if vim.fn.sha256(vim.fn.readblob(archive)) ~= want then
        return fail('checksum mismatch for ' .. archive_name)
      end
      run({ 'tar', '-xf', archive, '-C', partial, '--strip-components', '1' }, function(unpacked)
        if unpacked.code ~= 0 then
          return fail('could not unpack ' .. archive_name)
        end
        vim.fn.delete(archive)
        vim.fn.delete(sums)
        vim.fn.delete(final, 'rf')
        local moved, why = vim.uv.fs_rename(partial, final)
        if not moved then
          return fail('could not move the download into place: ' .. tostring(why))
        end
        local exe = vim.fs.joinpath(final, 'bin', EXE)
        info(exe, function(decoded, reason)
          if not decoded then
            vim.fn.delete(final, 'rf')
            return callback(nil, reason)
          end
          callback({ path = exe, source = 'download', info = decoded })
        end)
      end)
    end)
  end)
end

local function lookup(done)
  local function from_download()
    download(function(result, reason)
      if result then
        vim.notify('Swara: ready', vim.log.levels.INFO)
        return done(result)
      end
      local message = ('Swara: download failed (%s); LilyPond files stay plain text until the next start'):format(
        reason
      )
      vim.notify(message, vim.log.levels.WARN)
      done(nil, message)
    end)
  end

  local function from_data()
    local exe = vim.fs.joinpath(folder(), version, 'bin', EXE)
    if vim.fn.executable(exe) ~= 1 then
      return from_download()
    end
    info(exe, function(decoded)
      if decoded then
        return done({ path = exe, source = 'download', info = decoded })
      end
      from_download()
    end)
  end

  local on_path = vim.fn.exepath('swara')
  if on_path == '' then
    return from_data()
  end
  info(on_path, function(decoded)
    if decoded then
      return done({ path = on_path, source = 'path', info = decoded })
    end
    from_data()
  end)
end

--- Calls `callback` with `{ path, source = 'path' | 'download', info }`, where `info` is the decoded
--- output of `swara info`, or with nil and a message. The lookup runs once per Neovim session, and
--- callers that arrive while it runs wait for the same answer.
function M.resolve(callback)
  if found then
    return callback(found)
  end
  if failed then
    return callback(nil, failed)
  end
  if waiting then
    table.insert(waiting, callback)
    return
  end
  waiting = { callback }
  lookup(function(result, message)
    local callbacks = waiting
    waiting = nil
    found, failed = result, message
    for _, each in ipairs(callbacks) do
      each(result, message)
    end
  end)
end

return M
