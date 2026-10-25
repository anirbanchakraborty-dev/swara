# Skim refresh spike

After each save the plugin rebuilds the PDF, and the copy open in Skim has to show the new pages without anyone touching Skim and without Skim taking focus from the terminal. Skim can notice a changed file by itself, or it can be told to reload one through AppleScript. This spike measured both routes and checked focus at every step. Everything here was measured on 2026-10-24 with Skim 1.7.17 and LilyPond 2.26.0, on the machine described in `speed-basis.md`. The test PDF alternated between the generated song at 40 bars (2 pages) and at 80 bars (4 pages), so that every rebuild changed the page count Skim reports. Skim's preferences were read and never changed.

## The verdict

Skim watches the file when its preferences allow it, and the plugin asks Skim to revert when they do not. Both routes take about an eighth of a second and neither moves focus.

| Route | When | Latency | Focus |
| --- | --- | --- | --- |
| Skim watches the file | both reload preferences are on | median 131 ms after LilyPond exits (125 ms to 135 ms, six runs) | unchanged |
| The plugin asks Skim to revert | either preference is off | median 115 ms for the whole `osascript` call (111 ms to 124 ms, six runs) | unchanged |
| First open with `open -g -a Skim` | the first successful build | 4,056 ms with Skim not yet running | unchanged |

## The preferences

Two preferences decide whether Skim reloads by itself: `SKAutoCheckFileUpdate`, which makes it notice a changed file, and `SKAutoReloadFileUpdate`, which makes it reload without asking. Both read 1 on this machine. Skim ships with both set to false, in `InitialUserDefaults.plist` inside the application, so a fresh installation takes the revert route.

The health check reads them with `defaults read -app Skim SKAutoReloadFileUpdate` and `defaults read -app Skim SKAutoCheckFileUpdate`. Each prints 1 or 0 when the key has been set. When it never has, the command prints `Error: Could not find key '<key>' in domain 'net.sourceforge.skim-app.skim'.` and exits with status 1, which means Skim's shipped value, false. The plugin should take the watch route only when both read 1. With checking off, Skim has no reason to notice the change at all. That rule was read from the preferences rather than tested, because testing it means turning a preference off, which this spike does not do. For the same reason, what Skim does when it checks but does not reload, presumably asking whether to reload, was not observed.

## Opening without focus

The frontmost application was read before and after each step with `lsappinfo front`, which names it by its LaunchServices identifier and, unlike asking System Events, needs no Automation permission. `open -g -a Skim <pdf>` launched Skim and opened the PDF 4,056 ms later, and the frontmost application was the same before and after. The figure includes Skim's own launch. Later builds never reopen the file, so the cost is paid once.

## The watch route

Each run rebuilt the PDF with the other bar count and timed the moment Skim reported the new page count, counted from the moment LilyPond exited. Asking Skim through a fresh `osascript -e` call takes 110 ms to 158 ms each time (median 120 ms over ten calls), which would swamp the figure being measured. The timing therefore comes from a single JavaScript for Automation process that asks Skim for the page count about every 5 ms, a request costing 3.3 ms inside one process, and prints the time whenever the count changes.

Over six runs Skim showed the new count a median of 131 ms after LilyPond exited, between 125 ms and 135 ms. LilyPond finished writing the PDF 6 ms or 7 ms before it exited, so nearly all of the 131 ms is Skim noticing the file and reloading it. An earlier run that asked through a fresh `osascript` call each time agreed, at a median of 130 ms. Frequent asking could slow Skim slightly, so the figure is if anything an overestimate.

In one of the twelve watch runs the frontmost application changed, from another application to MuseScore Studio. Skim's own identifier never appeared, so that was a person switching applications during the run, not Skim taking focus.

## The revert route

Each run rebuilt the PDF and then ran `osascript -e 'tell application "Skim" to revert document 1'`. The whole call took a median of 115 ms, between 111 ms and 124 ms, and Skim reported the new page count as soon as it returned, in all six runs. Starting `osascript` accounts for almost all of that time. Because the watch preferences were on, Skim's own reload may have raced the revert, but the call's time is what the plugin waits for either way. The frontmost application was unchanged in all six runs.

`document 1` is the frontmost Skim document, which is the wrong one whenever more than one PDF is open, so the plugin addresses the document by its path instead. Skim's dictionary, `Skim.sdef` inside the application, gives the document class a `file` property and a hidden `path` property that holds the POSIX path as text. The form the plugin sends passes the path as an argument rather than writing it into the script, so no quoting is involved:

```bash
osascript -e 'on run argv' -e 'tell application "Skim" to revert (documents whose path is (item 1 of argv))' -e 'end run' <pdf>
```

It was checked on 2026-10-24 against a PDF opened with `open -g -a Skim`, in a folder whose name held a space and in one whose name was Bengali: both reverted with exit status 0, and the same form with `count` found exactly one document. A path Skim does not have open also exits 0 and changes nothing. Skim stores the resolved path: a PDF opened as `/tmp/…/t.pdf` was held as `/private/tmp/…/t.pdf`, and the unresolved path matched no document. The plugin therefore resolves the path with `vim.uv.fs_realpath` before sending it.

## Reproducing

`spikes/skim/run.py` builds the test PDF, drives Skim and prints the figures. It is run from the repository root with `python3 -I`, and writes its scratch files to `$TMPDIR/swara-skim-spike`.

```bash
python3 -I spikes/skim/run.py open
python3 -I spikes/skim/run.py idle 10
python3 -I spikes/skim/run.py watch 6
python3 -I spikes/skim/run.py revert 6
```
