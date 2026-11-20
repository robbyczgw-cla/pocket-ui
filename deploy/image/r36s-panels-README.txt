Black screen? Your R36S has one of the newer screens (2024 and later). Nothing is broken.

1. Put this card into a computer and open the drive called ROCKNIX.
2. Copy ONE file from this folder (r36s-panels) into the folder "overlays" and rename the copy to
   mipi-panel.dtbo
3. Put the card back into the R36S and start it again. Still black? Delete overlays/mipi-panel.dtbo and try the next file.
4. To go back to the original screen, delete overlays/mipi-panel.dtbo.

Which file first? r36s-panel4 fits most R36S from late 2024 on, then r36s_clone, rg351mp-v2, rg351v-v2, r36s-panel1,
r36s-panel2 and r36s-panel3. A wrong file only gives a black or garbled picture; it changes nothing else.

Still have the R36S's original card? Then the card can find the right screen by itself: copy the .dtb file from the original
card's first drive (for example rk3326-r35s-linux.dtb or rf3536k3ka.dtb) into the folder "stock" on this card's ROCKNIX drive
and start the R36S. It writes overlays/mipi-panel.dtbo, starts again once, and notes what it did in stock/panel-log.txt.

These files are ROCKNIX's prebuilt screen overlays (github.com/stolen/rnix, release "panel_overlays").
