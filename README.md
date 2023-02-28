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

### Cargo

```bash
cargo install --locked hyprland-autoname-workspaces
```

## Usage

```bash
$ ./hyprland-autoname-workspaces
```

or to dedup icon

```bash
$ ./hyprland-autoname-workspaces --dedup
```

## Configuration

Edit the mapping of applications class -> icon in the config file `~/.config/hyprland-autoname-workspaces/config.toml`.
No need to restart the applications then, there is an autoreload.

_Hint_: You can use glyphsearch and copy the unicode icon of your font for example https://glyphsearch.com/?query=book&copy=unicode
