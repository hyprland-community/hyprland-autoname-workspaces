# 🪟 hyprland-autoname-workspaces

![](https://img.shields.io/crates/d/hyprland-autoname-workspaces)
![](https://img.shields.io/github/issues-raw/cyrinux/hyprland-autoname-workspaces)
![](https://img.shields.io/github/stars/cyrinux/hyprland-autoname-workspaces)
![](https://img.shields.io/aur/version/hyprland-autoname-workspaces-git)
![](https://img.shields.io/crates/v/hyprland-autoname-workspaces)

This is a toy for Hyprland.

This app automatically rename workspaces with icons of started applications - tested with _waybar_.

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

or to dedup icon

```bash
$ hyprland-autoname-workspaces --dedup
```

## Configuration

In the config file `~/.config/hyprland-autoname-workspaces/config.toml`.

_You can use regex everywhere, and its case sensitive by default_

Edit the mapping of applications with `class = "icon"` in the `[icons]` part.

- You can exclude applications in the `[exclude]` with `class = title`.

In the `exclude` part, the key is the window `class`, and the value the `title`.
You can use `""` in order to exclude window with empty title and `"*"` as value to match all title of a class name.

Example:

```
...
[exclude]
fcitx = ".*"
[Ss]team = "Friends list.*"
```

- You can match on title with `[title.classname]` with `"a word in the title" = "icons"`.

Example:

```
...
[title."(?i)kitty"]
"(?i)neomutt" = "neomutt"
```

No need to restart the applications then, there is an autoreload.

_Hint_: You can use glyphsearch and copy the unicode icon of your font for example https://glyphsearch.com/?query=book&copy=unicode

_Hint_: You can find hyprland class names for currently running apps using: `hyprctl clients  | grep -i class`, or you can also use `hyprland-autoname-workspaces --verbose`.
