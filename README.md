# Swara

Swara is a fast LilyPond toolkit for Neovim, built from the ground up. It has three parts: a new Tree-sitter grammar for LilyPond, a language server written in Rust that checks a score each time typing pauses, and a Neovim plugin that wires both in, opens the PDF in Skim, previews and plays a score, and creates guitar, piano and song lessons that print Indian sargam names and Bengali lyrics.

## What it does

Opening a `.ly` or `.ily` file starts Swara's colours, folds and indent from its own grammar, and its language server, which does the following.

- Checks the text after every edit, within a few milliseconds, for mistakes the grammar can see: unclosed braces and other syntax errors, commands nothing defines, note names the file's note language does not have, and `\include` lines that name no file.
- Runs LilyPond's own check once typing pauses, so LilyPond's errors and warnings appear on the lines they name without saving the file.
- Tidies the file on each save, and on request through Neovim's formatting.
- Completes LilyPond commands after a backslash, shows a command's description on hover, and jumps to where a variable is defined.
- Lists the file's variables and its `\score`, `\book` and `\bookpart` blocks as document symbols, for a picker or an outline.
- Builds the PDF and MIDI on each save, opens the PDF the first time, and has Skim reload it after every later build.

LilyPond runs any Scheme code a file contains, and that code can run programs and change files as the user. So opening a file never runs LilyPond. The first run for a file comes only after typing in it, saving it or asking for a build, each a deliberate act on that file, the same as pressing compile. A downloaded piece opened only to read is never run, and the `auto_check` setting turns the check while typing off altogether.

The plugin adds the rest. It shows the score as an image inside the editor, plays it through FluidSynth, opens it in MuseScore for slower practice, and writes new lessons from templates. A guitar or piano lesson prints the sargam name of each note (Sa, Re, Ga and so on) under the staff, in Bengali, Devanagari or Latin letters, counted from any chosen Sa. A song lesson takes lyrics typed in English letters and prints them in Bengali script, with the words as typed in smaller type beneath them, or either line alone. The converter is a general engine, and Bengali is the one script it carries a table for so far.

## Requirements

- Neovim 0.12 or later.
- LilyPond, for the LilyPond check, builds, preview and playback. Swara is tested with LilyPond 2.26.0. Other versions should work, because the server reads LilyPond's commands from whichever LilyPond is installed. Colours, the instant checks and tidying work without it.
- FluidSynth, for playback, and a soundfont. MuseScore 4's `MS Basic.sf3` is used when MuseScore 4 is installed in `/Applications`, and the `soundfont` setting names any other.
- MuseScore 4, for `:Swara musescore`.
- On macOS, Skim, for a PDF that reloads after each build. Without it the PDF opens in the default viewer.
- snacks.nvim, optionally, to draw the preview inside a terminal that can show images. Without it the preview opens in the system's image viewer.

On macOS, Homebrew installs all of these but snacks.nvim.

```bash
brew install neovim lilypond fluid-synth
```

```bash
brew install --cask musescore skim
```

## Installing

With [lazy.nvim](https://github.com/folke/lazy.nvim):

```lua
{ 'anirbanchakraborty-dev/swara', ft = 'lilypond', opts = {} }
```

The plugin needs the `swara` program, which holds the language server and the grammar. It uses a `swara` found on `PATH` first. Otherwise, the first time a LilyPond file opens, it downloads the release archive for its own version from this repository's GitHub releases into Neovim's data folder, and unpacks it only once its SHA-256 matches the release's `SHA256SUMS`. Release archives are built for macOS on Apple silicon and Intel, Linux on x86-64 and ARM64, and Windows on x86-64. The tests run on macOS and Linux.

### With nvim-treesitter

nvim-treesitter is not needed, because the plugin loads its grammar from the `swara` program. To have nvim-treesitter manage the grammar instead, for example to use it in other plugins, register it from [tree-sitter-lilypond](https://github.com/anirbanchakraborty-dev/tree-sitter-lilypond) in a `User TSUpdate` autocommand, as nvim-treesitter's `main` branch documents, and run `:TSInstall lilypond` once.

```lua
vim.api.nvim_create_autocmd('User', { pattern = 'TSUpdate',
  callback = function()
    require('nvim-treesitter.parsers').lilypond = {
      install_info = {
        url = 'https://github.com/anirbanchakraborty-dev/tree-sitter-lilypond',
        revision = 'v0.1.0',
        queries = 'queries',
      },
    }
  end,
})
```

Keep the revision at the grammar tag that matches the plugin's release, since Swara's queries are written for that grammar. A different LilyPond grammar installed under the name `lilypond` stops Swara's colours, and `:checkhealth swara` names the conflict.

## Commands

| Command | What it does |
| --- | --- |
| `:Swara new guitar <name>` | Writes a guitar lesson `<name>.ly` in the current folder, with the sargam include beside it, and opens it |
| `:Swara new piano <name>` | Writes a piano lesson the same way |
| `:Swara new song <name>` | Writes a song lesson with its lyrics file and the lyrics include |
| `:Swara build` | Builds the PDF and MIDI from the text in the editor, saved or not |
| `:Swara open` | Opens the last build's PDF |
| `:Swara preview` | Shows the score as an image inside the editor |
| `:Swara play` | Plays the last build's MIDI through FluidSynth |
| `:Swara stop` | Stops playing |
| `:Swara musescore` | Opens the last build's MIDI in MuseScore |

## Keys

Each key is set in LilyPond buffers only, after the local leader key, which Neovim's `maplocalleader` setting chooses and which is a backslash unless changed. The `keys` setting turns them all off.

| Key | What it does |
| --- | --- |
| `<localleader>o` | Opens the PDF |
| `<localleader>v` | Previews the score in the editor |
| `<localleader>p` | Plays the score |
| `<localleader>s` | Stops playing |
| `<localleader>m` | Opens the score in MuseScore |
| `<localleader>b` | Builds the PDF and MIDI |

## Settings

Every setting goes in `opts` for lazy.nvim, or in `require('swara').setup({ ... })` otherwise. The defaults suit most uses.

| Setting | Default | What it does |
| --- | --- | --- |
| `auto_check` | `true` | Runs LilyPond's own check each time typing pauses |
| `format_on_save` | `true` | Tidies the file each time it is saved |
| `keys` | `true` | Sets the keys above in LilyPond buffers |
| `soundfont` | MuseScore 4's `MS Basic.sf3` | The `.sf2` or `.sf3` soundfont that playback uses |
| `debounce_ms` | `10` | How long Neovim waits after a change before sending it to the server, in milliseconds |
| `preview_dpi` | `120` | The resolution of the preview image |
| `lilypond` | the `lilypond` on `PATH` | The LilyPond program to use |
| `include_paths` | `{}` | More folders to search for `\include` files, after the file's own folder |
| `release_base_url` | this repository's GitHub releases | Where the plugin downloads `swara` from |

`:checkhealth swara` reports the `swara` program and where it came from, the grammar, LilyPond, the command cache, FluidSynth and the soundfont, MuseScore and, on macOS, whether Skim reloads a changed PDF by itself. Each thing that is missing or off comes with its fix. The check only reads. Where Skim's own reload is off, it shows the two `defaults write` commands that turn it on, and Swara has Skim reload the PDF after each build in the meantime.

## Speed

Each target comes from the design, and every one was met with LilyPond 2.26.0 on the machine described in [docs/spikes/speed-basis.md](docs/spikes/speed-basis.md). [docs/bench/speed.md](docs/bench/speed.md) gives the fastest and slowest runs and how each figure is taken.

| Measurement | Median | Target |
| --- | --- | --- |
| Colours after a keystroke | 0.19 ms | under 16 ms |
| Instant checks after an edit | 1.2 ms | under 50 ms |
| LilyPond check after the last edit, 100-bar song | 551 ms | under 600 ms |
| Tidy, 100-bar song | 1.1 ms | under 10 ms |
| Completion after `\` | 1.1 ms | under 20 ms |
| Server ready | 2.3 ms | under 100 ms |
| PDF after save, 4-bar lesson | 266 ms | about 300 ms |
| PDF after save, 100-bar song | 1,311 ms | about 1,500 ms |
| Play, first time, to the first note | about 1,030 ms | about 1,500 ms |
| Play, later, to the first note | about 4 ms | under 200 ms |

The LilyPond check has the least room: 330 ms of it is the pause Swara waits for typing to stop, and most of the rest is LilyPond itself. The two play figures run to the first note Swara sends, and FluidSynth's audio buffer adds a delay after that which was not measured.

## Building from source

The workspace expects the grammar repository checked out beside it, as `../tree-sitter-lilypond`, and needs Rust 1.98.

```bash
cargo build --release -p swara -p swara-parser
```

```bash
cargo test --workspace
```

```bash
nvim --clean --headless -l tests/run.lua
```

## Licence

Swara is released under the MIT licence, in [LICENSE](LICENSE).

Two boundaries keep it that way. First, LilyPond's command descriptions, which completion and hover show, are GPL text, so Swara never ships them. The server reads them from the LilyPond installed on each machine the first time it needs them and keeps them in that user's cache folder. Second, no code, query, word list or description is copied from a GPL project, LilyPond, Frescobaldi, python-ly and nvim-lilypond-suite among them. The grammar, the queries and the test files were written from scratch for this project.
