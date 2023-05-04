#!/usr/bin/env python3
"""
This script pulls the font awesome spec from their repo on github
then generate an [icon] list for `hyprland-autoname-workspaces`.
> Taken from fontawesome-python.
"""

import argparse

import requests
import yaml


def unicode_symbol(hex_code):
    """
    Convert a hexadecimal code to its corresponding Unicode character.

    :param hex_code: The hexadecimal code of the Unicode character.
    :return: The Unicode character.
    """
    return chr(int(hex_code, 16))


def print_custom_icons():
    """
    Print the custom icon for Alacritty.
    """
    term_icon = "f120"
    print(f'"(?i)alacritty" = "{unicode_symbol(term_icon)}"')
    print(f'"(?i)kitty" = "{unicode_symbol(term_icon)}"')


def print_icon_names(icons_dict, include_aliases):
    """
    Print the icon names and their corresponding Unicode symbols
    from the provided icons dictionary.

    :param icons_dict: The dictionary containing icon information.
    :param include_aliases: Whether to include aliases in the output.
    """
    for icon_name, icon in icons_dict.items():
        # Create the names list with the icon_name and its aliases if required
        names = [icon_name] + (
            icon["search"]["terms"]
            if include_aliases and icon["search"]["terms"] is not None
            else []
        )

        # Iterate through the names, filtering out empty strings
        for name in filter(lambda x: x != "", names):
            print(f'"(?i){name}" = "{unicode_symbol(icon["unicode"])}"')


def main(uri, version, include_aliases):
    """
    Main function to fetch icons dictionary from the provided URI and print
    the icon names and their corresponding Unicode symbols.

    :param uri: The URI to fetch the icons dictionary.
    :param version: The version of the icons (default to 'master').
    :param include_aliases: Whether to include aliases in the output.
    """
    icons_dict = yaml.full_load(requests.get(uri).text)

    print("[icons]")

    # Custom icons
    print_custom_icons()

    # Icons from the provided dictionary
    print_icon_names(icons_dict, include_aliases)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate icons.py, containing a python mapping for font awesome icons"
    )
    parser.add_argument(
        "--revision",
        help="Version of font of font awesome to download and use. Should correspond to a git branch name.",
        default="master",
    )
    parser.add_argument(
        "--include_aliases",
        help="If enabled, also adds aliases for icons in the output.",
        action="store_true",
    )
    args = parser.parse_args()

    REVISION = args.revision
    URI = (
        "https://raw.githubusercontent.com"
        f"/FortAwesome/Font-Awesome/{REVISION}/metadata/icons.yml"
    )

    main(URI, args.revision, args.include_aliases)
