# hyprland-autoname-workspaces

This is a toy for Hyprland.

This app automatically rename workspaces with icons of started applications.

# Install

Available as AUR package under the name `hyprland-autoname-workspaces-git`.

# Usage

Edit `~/.config/hyprland-autoname-workspaces/config.toml` to add class to icon mapping.
As toml file don't support dots in key, take care to replace "." by "-" in class name.

```bash
$ ./hyprland-autoname-workspaces
```

or to dedup icon

```bash
$ ./hyprland-autoname-workspaces --dedup
```
