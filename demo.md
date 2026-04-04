# Demos

Every command, every option, every error case. One action per demo.

## Patterns

### Hex prefix
`git vanity cafe`
![](demo/01-prefix.svg)

### Preset hex word
`git vanity -p coffee`
![](demo/02-preset.svg)

### Random preset
`git vanity -r`
![](demo/03-random.svg)

### Repeat pattern
`git vanity repeat:3` — any 3 identical adjacent chars
![](demo/04-repeat.svg)

### Pair pattern
`git vanity xx` — any 2 identical adjacent chars
![](demo/05-pair.svg)

### Structured pattern
`git vanity aaxxx` — prefix `aa` + 3 identical chars
![](demo/06-structured.svg)

### Regex pattern
`git vanity "/^dead/"`
![](demo/07-regex.svg)

## Match Position

### End of hash
`git vanity dead -m end`
![](demo/08-match-end.svg)

### Anywhere in hash
`git vanity cafe -m contains`
![](demo/09-match-contains.svg)

## Options

### Dry run — preview before writing
`git vanity cafe -n`
![](demo/10-dry-run.svg)

### Debug — show speed and stats
`git vanity cafe -d`
![](demo/11-debug.svg)

### Quiet vs normal — progress bar comparison
Without `-q` shows progress bar, with `-q` silent until done
![](demo/12-quiet-vs-normal.svg)

### Threads — control parallelism
`git vanity cafe -j 2 -d`
![](demo/13-threads.svg)

### Timeout — abort after time limit
`git vanity 00000000 -t 2000`
![](demo/14-timeout.svg)

### Max attempts — abort after N tries
`git vanity 00000000 --max-attempts 1000`
![](demo/15-max-attempts.svg)

### Message override
`git vanity cafe --message "new message"`
![](demo/16-message.svg)

### No repeat — disable structured detection
`git vanity aaxxx --no-repeat`
![](demo/17-no-repeat.svg)

### List presets — all available hex words
`git vanity --list-presets`
![](demo/18-list-presets.svg)

## Subcommands

### Show — inspect current commit
`git vanity show`
![](demo/19-show.svg)

### Log — vanity stats for repo
`git vanity log`
![](demo/20-log.svg)

### Undo — strip nonce, restore original hash
`git vanity undo`
![](demo/21-undo.svg)

## Error Cases

### Invalid pattern
`git vanity xyz`
![](demo/22-invalid-pattern.svg)

### Unknown preset
`git vanity -p banana`
![](demo/23-invalid-preset.svg)

### Invalid match position
`git vanity cafe -m middle`
![](demo/24-invalid-position.svg)

### No pattern specified
`git vanity`
![](demo/25-no-pattern.svg)

### Not a git repo
`git vanity cafe` (outside a repo)
![](demo/26-not-a-repo.svg)
