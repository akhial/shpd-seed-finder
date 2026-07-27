# Shattered Pixel Dungeon chest artwork

The Seed Seeker application icon is a recoloured derivative of the Shattered
Pixel Dungeon launcher icon, specifically the chest in:

`android/src/main/res/mipmap-xxxhdpi/ic_launcher_foreground.png`

from Shattered Pixel Dungeon v3.3.8, commit
`7b8b845a76fe76c6b7c031ae9e570852411f56db`.

Seed Seeker changes only the palette. Upstream's nine tones are mapped by
luminance onto a blueprint scheme — the gold ironwork becomes white linework,
the green panels become blues. No pixel is moved, added or removed, so the
geometry is upstream's.

The one exception is the reduced chest in `seed-seeker-small.svg`, used at 48px
and below where the full sprite cannot land on whole pixels. It is an original
hand-drawn reduction rather than a transcription, following the same
composition, and is a derivative work of the same original.

- Pixel Dungeon: Copyright © 2012–2015 Oleg Dolya
- Shattered Pixel Dungeon: Copyright © 2014–2026 Evan Debenham
- Project: https://github.com/00-Evan/shattered-pixel-dungeon
- License: GNU General Public License v3.0 or later (see `../../COPYING`)

Seed Seeker is an independent, unofficial project. It is not affiliated with or
endorsed by Shattered Pixel Dungeon or its authors. Because an application icon
is an identity mark rather than in-app artwork, the recolouring is deliberate:
the blueprint palette is intended to read as a distinct mark at a glance in a
launcher or a store listing, not as the game itself.
