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
Edit the mapping of applications with `class = "icon"` in the `[icons]` part.
You can also exclude applications in the `[exclude]` with `class = title`.

In the `exclude` part, the key is the window `class`, and the value the `title`.
You can use `""` in order to exclude window with empty title and `"*"` as value to match all title of a class name.

Example:

```
...
[exclude]
fcitx = "*"
Steam = "Friends list"
```

No need to restart the applications then, there is an autoreload.

_Hint_: You can use glyphsearch and copy the unicode icon of your font for example https://glyphsearch.com/?query=book&copy=unicode
