#
# Copyright 2019 Joyent, Inc.
#

# Create a DVI from fastpacket.tex and then convert that DVI to into an SVG for
# use in the rustdocs
#
# Building this requires a working TeX installation. Here are a few examples of how to do this:
#
# Mac
#
# $ brew cask install mactex-no-gui
# $ export PATH=/Library/TeX/texbin:$PATH
# $ cd docs ; make
#
# Arch Linux
#
# $ sudo pacman -S texlive-most
# $ cd docs ; make

.PHONY: all
all:
	latex fastpacket && dvisvgm --no-fonts fastpacket.dvi fastpacket.svg
