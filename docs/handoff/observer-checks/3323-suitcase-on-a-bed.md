# Is a suitcase lying on a bed the crosshair's target, from every aim at it?

Priority: P3
Needs: the window in front (HUD prompts, E); no particular save
Spec: #3323, [simulation.md](../../spec-notes/simulation.md) §5 crosshair activation

A player reports the Suitcase 0x000AFA24 lying on BedQueenRight 0x000D72C7 in BisonSteve02
can be looted in the original [reported: the player's issue, not captured]. Ours since
#3323: a bed's boxes are set aside when one of them wholly holds, along the ray, the box of
something the player can activate, entered before the first collision hit; otherwise the
nearest box or collision hit wins [design].

Stand beside that bed and sweep the crosshair over the suitcase's lid, its side, its edge
against the mattress, a corner hanging over the bed's edge if there is one, and the bare
mattress beside it. At each aim write down the HUD prompt (Open Suitcase or the bed's) and
what E does (the suitcase's container menu or the bed's sleep/wait menu); log
`GetCrosshairRef` separately, as it may name another reference than the prompt.

Record which aims give the suitcase and which the bed, without reading a rule into them
yet; compare with ours afterwards (aim for aim in the same spot in NVForever).
