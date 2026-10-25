if vim.g.loaded_swara then
  return
end
vim.g.loaded_swara = true

vim.api.nvim_create_autocmd('FileType', {
  group = vim.api.nvim_create_augroup('swara', { clear = true }),
  pattern = 'lilypond',
  desc = 'Attach Swara to LilyPond buffers',
  callback = function(args)
    require('swara').attach(args.buf)
  end,
})

vim.api.nvim_create_user_command('Swara', function(opts)
  require('swara.commands').run(opts)
end, {
  nargs = '+',
  desc = 'Swara: new, build, open, preview, play, stop or musescore',
  complete = function(arglead, cmdline, cursorpos)
    return require('swara.commands').complete(arglead, cmdline, cursorpos)
  end,
})
