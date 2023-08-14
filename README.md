# 🪟 hyprland-autoname-workspaces

![](https://img.shields.io/crates/d/hyprland-autoname-workspaces)
![](https://img.shields.io/github/issues-raw/hyprland-community/hyprland-autoname-workspaces)
![](https://img.shields.io/github/stars/hyprland-community/hyprland-autoname-workspaces)
![](https://img.shields.io/aur/version/hyprland-autoname-workspaces-git)
![](https://img.shields.io/crates/v/hyprland-autoname-workspaces)
[![Discord](https://img.shields.io/discord/1055990214411169892?label=discord)](https://discord.gg/zzWqvcKRMy)
[![codecov](https://codecov.io/gh/hyprland-community/hyprland-autoname-workspaces/branch/main/graph/badge.svg?token=NYY5DRMLM4)](https://codecov.io/gh/hyprland-community/hyprland-autoname-workspaces)

🕹️This is a toy for Hyprland.

This app automatically rename workspaces with icons of started applications - tested with _[waybar](https://github.com/Alexays/Waybar)_ and _[eww](https://github.com/elkowar/eww)_.

You have to set the config file with your prefered rules based on `class` and `title`. Regex (match and captures) are supported.

## FAQ, tips and tricks ❓

https://github.com/hyprland-community/hyprland-autoname-workspaces/wiki/FAQ

## Install

### AUR 📦

Available as AUR package under the program name `hyprland-autoname-workspaces-git`.
You can then use the service `systemctl --user enable --now hyprland-autoname-workspaces.service`.

### Nix 📦

Available in nixpkgs as [`hyprland-autoname-workspaces`](https://search.nixos.org/packages?channel=unstable&show=hyprland-autoname-workspaces).
You can add it to your `systemPackages` or try it without installing it with `nix run`.

```bash
$ nix run nixpkgs#hyprland-autoname-workspaces
```

### Cargo 📦

```bash
$ cargo install --locked hyprland-autoname-workspaces
```

## Usage

```bash
$ hyprland-autoname-workspaces
```

## Configuration

_For all parameters, check the `config.toml.example` in this repository._

The config file can be specified using the `-c <CONFIG>` option, otherwise it defaults to `~/.config/hyprland-autoname-workspaces/config.toml`. If you specify a path that doesn't exist, a default configuration file will be generated.

_You can use regex everywhere, and its case sensitive by default_

Edit the mapping of applications with `class = "icon"` in the `[icons]` part.

In icons value, you can use the placeholders `{class}`, `{title}` and `{match1}`, `{match2}` if you use regex captures.

Example:

```
[class]
DEFAULT = "{class}: {title}"
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

- You can match on title with `[title_in_class.classname]` and `[title_in_class_active.class]` with `"a word in the title" = "icons"`.

_Hint_: There is also `title_in_initial_class`, `initial_title_in_class`, `initial_title_in_initial_class` and so on.

Example:

```
...
[title."(xterm|(?i)kitty|alacritty)"]
"(?i)neomutt" = "mail"
ncdu = "file manager"

[title."(firefox|chrom.*)"]
youtube = "yt"
google = "gg"

[title_active."(firefox|chrom.*)"]
youtube = "<span color='red'>yt</span>"
google = "<span color='blue'>{icon}</span>"
...

```

- You can deduplicate icons with the `dedup` parameter in the `root` section of config file.

```
dedup = true
dedup_inactive_fullscreen = true
...
[title."(xterm|(?i)kitty|alacritty)"]
"(?i)neomutt" = "mail"
ncdu = "file manager"
...
```

- You can also redefine all the default formatter with those `[format]` section formatters parameters.
  The available list of `{placeholder}` is:

workspace:

- client
- id (or id_long)
- name (use value from `[workspaces_name]` mapping)
- delim

clients:

- icon
- counter_s, counter_unfocused_s, counter, counter_unfocused
- class, iitle
- delim
- match1, match2, match3, matchN (for regex captures)

```
[format]
dedup = true
dedup_inactive_fullscreen = true
delim = " " # NARROW NO-BREAK SPACE
workspace = "<span color='red'>{id}:</span>{delim}{clients}"
workspace_empty = "<span color='red'>{id}</span>"
client = "{icon}{delim}"
client_active = "<span color="red">{icon}</span>{delim}"
client_dup = "{icon}{counter_sup}{delim}"
client_dup_fullscreen = "[{icon}]{delim}{icon}{counter_unfocused_sup}"
client_fullscreen = "[{icon}]{delim}"
...
```

See `config.toml.example` and the wiki for more example, feel free to share your config !

No need to restart the applications then, there is an autoreload.

_Hint_: You can use glyphsearch and copy the unicode icon of your font for example https://glyphsearch.com/?query=book&copy=unicode

_Hint_: You can find hyprland class names for currently running apps using: `hyprctl clients  | grep -i class`, or you can also use `hyprland-autoname-workspaces --verbose`.

_Hint_: Feel free to adapt and use this [script](https://github.com/Psykopear/i3autoname/blob/master/scripts/generate_icons.py) to generate your config file. This is untested for the moment.

_Hint_: You can bootstrap your `[icons]` with the `contrib/generate_icons.py` script.

_Hint_: All styling param that you can use with `<span>` are here: https://docs.gtk.org/Pango/pango_markup.html
