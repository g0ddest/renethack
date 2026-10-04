# The help's sources

The in-game help (F1, the HUD's `?` button) shows NetHack's Guidebook in
the interface's language, by chapters, with a search. `make help` builds
`guidebook.en.json` and `guidebook.ru.json` from the sources below with
`tools/help/guidebook.py`; `make help-check` fails when they are stale.

## English

`engine/upstream/doc/Guidebook.mn`: "A Guide to the Mazes of Menace
(Guidebook for NetHack)", original version by Eric S. Raymond, edited and
expanded for NetHack 5.0 by Mike Stephenson and others. Part of NetHack,
under the NetHack General Public License (`engine/upstream/dat/license`).

## Russian

`guidebook-ru.md`: «Руководство по Грозным Лабиринтам», the Russian
translation of the Guidebook (of NetHack 3.4) from
<https://github.com/velikodniy/nethack-guide> at commit 576aa83
(2014-09-24), checked by its sha256 in `help.lock.json`. Translated on
notabenoid.com (as of 2014-02-01; translator witmolif@gmail.com); edited
and typeset by Vadim Velikodniy.

The repository has no licence file. A translation of NetHack's
documentation, it is distributed here under the NetHack General Public
License, as the Guidebook is.
