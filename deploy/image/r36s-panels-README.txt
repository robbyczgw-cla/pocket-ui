Black screen? Your R36S has one of the newer screens (2024 and later). Nothing is broken.

1. Put this card into a computer and open the drive called ROCKNIX.
2. Copy ONE file from this folder (r36s-panels) into the folder "overlays" and rename the copy to
   mipi-panel.dtbo
3. Put the card back into the R36S and start it again. Still black? Delete overlays/mipi-panel.dtbo and try the next file.
4. To go back to the original screen, delete overlays/mipi-panel.dtbo.

Which file first? Find your board name (for example R36S V22). It is printed on the board itself, so you may need to
open the back. Then try the file in this table first:

  Your R36S                                          File to try first
  R35S V11, R36S V12 (older screen, if black)       rg351v-v2
  R36S V12 (newer screen), HL-R36H V20 and V21,
  R36S V21 and V22, R36XX V21                        r36s-panel4
  R36S V30                                           rg351mp-v2
  R36S Y02, RG36S Y07                                r36s_clone

This is the closest file we found, not a promise. If the picture is black, garbled or shifted, delete the copy and try the
next one: r36s-panel4, r36s_clone, rg351mp-v2, rg351v-v2, r36s-panel1, r36s-panel2, r36s-panel3. A wrong file only gives
a bad picture; it changes nothing else. Another board, or a clone? Then no file here is known to fit. The safe way is the
original card.

Still have the R36S's original card? Then the card can find the right screen by itself: copy the .dtb file from the original
card's first drive (for example rk3326-r35s-linux.dtb or rf3536k3ka.dtb) into the folder "stock" on this card's ROCKNIX drive
and start the R36S. It writes overlays/mipi-panel.dtbo, starts again once, and notes what it did in stock/panel-log.txt.

These files are ROCKNIX's prebuilt screen overlays (github.com/stolen/rnix, release "panel_overlays").
