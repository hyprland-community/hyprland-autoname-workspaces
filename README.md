# 🪟 hyprland-autoname-workspaces

![](https://img.shields.io/crates/d/hyprland-autoname-workspaces)
![](https://img.shields.io/github/issues-raw/hyprland-community/hyprland-autoname-workspaces)
![](https://img.shields.io/github/stars/hyprland-community/hyprland-autoname-workspaces)
![](https://img.shields.io/aur/version/hyprland-autoname-workspaces-git)
![](https://img.shields.io/crates/v/hyprland-autoname-workspaces)
[![Discord](https://img.shields.io/discord/1055990214411169892?label=discord)](https://discord.gg/zzWqvcKRMy)

🕹️This is a toy for Hyprland.

This app automatically rename workspaces with icons of started applications - tested with _[waybar](https://aur.archlinux.org/packages/waybar-hyprland-git)_.

You have to set the config file with your prefered rules based on `class` and `title`. Regex are supported.

## Install

### AUR

Available as AUR package under the program name `hyprland-autoname-workspaces-git`.
You can then use the service `systemctl --user enable --now hyprland-autoname-workspaces.service`.

### Cargo

```bash
$ cargo install --locked hyprland-autoname-workspaces
```

## Usage

```bash
$ hyprland-autoname-workspaces
```

## Configuration

In the config file `~/.config/hyprland-autoname-workspaces/config.toml`.

_You can use regex everywhere, and its case sensitive by default_

Edit the mapping of applications with `class = "icon"` in the `[icons]` part.

In icons value, you can use the placeholder `${class}` and `${title}`.

Example:

```
[icons]
DEFAULT = "${class}: ${title}"
...
```

- You can exclude applications in the `[exclude]` with `class = title`.

In the `exclude` part, the key is the window `class`, and the value the `title`.
You can use `""` in order to exclude window with empty title and `".*"` as value to match all title of a class name.

Example:

```
...
[exclude]
"(?i)fcitx" = ".*" # will match all title for fcitx
"[Ss]team" = "Friends list.*"
"[Ss]team" = "^$" # will match and exclude all Steam class with empty title (some popups)
```

- You can match on title with `[title.classname]` with `"a word in the title" = "icons"`.

Example:

```
...
[title."(xterm|(?i)kitty|alacritty)"]
"(?i)neomutt" = "mail"
ncdu = "file manager"

[title."(firefox|chrom.*)"]
youtube = "yt"
google = "gg"
...

```

- You can deduplicate icons with the `dedup` parameter in the `root` section of config file.

```
dedup = true
...
[title."(xterm|(?i)kitty|alacritty)"]
"(?i)neomutt" = "mail"
ncdu = "file manager"
...
```

No need to restart the applications then, there is an autoreload.

_Hint_: You can use glyphsearch and copy the unicode icon of your font for example https://glyphsearch.com/?query=book&copy=unicode

_Hint_: You can find hyprland class names for currently running apps using: `hyprctl clients  | grep -i class`, or you can also use `hyprland-autoname-workspaces --verbose`.

_Hint_: Feel free to adapt and use this [script](https://github.com/Psykopear/i3autoname/blob/master/scripts/generate_icons.py) to generate your config file. This is untested for the moment.

_Hint_: You can bootstrap your `[icons]` with the `contrib/generate_icons.py` script.
