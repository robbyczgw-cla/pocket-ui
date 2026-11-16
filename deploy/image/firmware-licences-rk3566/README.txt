Firmware inside SYSTEM (usr/lib/kernel-overlays/base/lib/firmware) and its terms: RK3566 images (RG DS Plus)
==========================================================================================================

ROCKNIX 20261101 puts these 293 files into SYSTEM for its RK3566 family (many handhelds and USB Wi-Fi/Ethernet
adapters; the RG DS Plus itself uses few of them). All are unchanged. Compared on 2026-11-05 with
linux-firmware 20260309 (https://cdn.kernel.org/pub/linux/kernel/firmware/linux-firmware-20260309.tar.gz,
sha256 28fb1b57fa9531edbd001a2462068267f10622e613abd40f952344b3f1fc5a16) and its WHENCE file, which names each file's
terms: 208 files are byte for byte upstream's, 73 more are upstream's "Link:" names whose bytes are the target's,
2 are the wireless regulatory database (wireless-regdb 2026.05.30), 2 are a Raspberry Pi board file under a second name.
The licence texts named below are in this folder (from that linux-firmware release) or next to it.

OPEN BEFORE ANY RELEASE (this is a private test image): 8 files are not traced (marked NOT TRACED below), among them
the RG DS Plus's speaker amplifier settings; ROCKNIX's repository states no licence for them. And the Mali GPU driver
in SYSTEM (libmali, Arm's closed driver as Rockchip publishes it) needs its terms read.

  md5                              file  source and terms
  78fe4478dca9134c028e7507421b3f6a ar5523.bin  identical to linux-firmware file; LICENCE.atheros_firmware
  cd95e83312eec65ff523025b52badb4a arm/mali/arch10.10/mali_csffw.bin  identical to linux-firmware file; LICENCE.mali_csffw
  cd95e83312eec65ff523025b52badb4a arm/mali/arch10.12/mali_csffw.bin  identical to linux-firmware file; LICENCE.mali_csffw
  cd95e83312eec65ff523025b52badb4a arm/mali/arch10.8/mali_csffw.bin  identical to linux-firmware file; LICENCE.mali_csffw
  64fc3695e92756f111f6bc7287ff1569 arm/mali/arch11.8/mali_csffw.bin  identical to linux-firmware file; LICENCE.mali_csffw
  15f238b54dbf8aadcbccab55c47c05ac arm/mali/arch12.8/mali_csffw.bin  identical to linux-firmware file; LICENCE.mali_csffw
  98bd8fc1e193f03917fb6e87a2b0529c arm/mali/arch13.8/mali_csffw.bin  identical to linux-firmware file; LICENCE.mali_csffw
  1211fa34c09e10ba48381586b7c3883d ath3k-1.fw  identical to linux-firmware file; LICENCE.atheros_firmware
  b7fefa808648a19418a6b03904aca98b ath6k/AR6004/hw1.2/bdata.bin  identical to linux-firmware file; LICENCE.atheros_firmware
  99db99515f8170efdb7aaf62d2e7ca93 ath6k/AR6004/hw1.3/bdata.bin  identical to linux-firmware file; LICENCE.atheros_firmware
  da399fbfc8017629d101646100c5b495 ath9k_htc/htc_7010-1.4.0.fw  identical to linux-firmware file; LICENCE.atheros_firmware
  c5b2dc29f7ec2f053169ada3b2ef70ae ath9k_htc/htc_9271-1.4.0.fw  identical to linux-firmware file; LICENCE.atheros_firmware
  aa6afbfc6537b13cb6c7f836f7589b12 aw88166_acf.bin  NOT TRACED: the RG DS Plus's speaker amplifier (Awinic AW88166) settings, from ROCKNIX's packages, no licence stated
  9911d6777367cdd3afcd4fba16c318d8 brcm/BCM-0a5c-6410.hcd  identical to linux-firmware brcm/BCM-0bb4-0306.hcd (WHENCE Link); LICENCE.cypress
  9911d6777367cdd3afcd4fba16c318d8 brcm/BCM-0bb4-0306.hcd  identical to linux-firmware file; LICENCE.cypress
  c53608f5818b702c46a012c57b4196ee brcm/bcm4329-fullmac-4.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  b0736e3590b05d27284fbb8a3efd50e1 brcm/bcm43xx-0.fw  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  5e51778ee011badcb42f1e2cb4ab3956 brcm/bcm43xx_hdr-0.fw  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  d193bd38eba449a80d1e7e35e07322aa brcm/brcmfmac43012-sdio.bin  identical to linux-firmware cypress/cyfmac43012-sdio.bin (WHENCE Link); LICENCE.broadcom_bcm43xx
  7afd2d83d31eff4f12d67858490cfc86 brcm/brcmfmac43012-sdio.clm_blob  identical to linux-firmware cypress/cyfmac43012-sdio.clm_blob (WHENCE Link); LICENCE.broadcom_bcm43xx
  dc78f76883f5a0df9f13fad30d78442b brcm/brcmfmac43143-sdio.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  2be9d4da43aba729bfa3417829e011df brcm/brcmfmac43143.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  f579673b5dc45640c814a9c68abcaf55 brcm/brcmfmac43236b.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  44b13e2afe822657634ca0f08bd8c584 brcm/brcmfmac43241b0-sdio.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  b6fe79781f3e2f4a30705f2ebe5e9c6c brcm/brcmfmac43241b4-sdio.Advantech-MICA-071.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  b219396d9341e6eabc9c4d6eb3e8a87c brcm/brcmfmac43241b4-sdio.Intel Corp.-VALLEYVIEW C0 PLATFORM.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  62dffd52291975a60b2c079f2b671c06 brcm/brcmfmac43241b4-sdio.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  6053b3e59ac12f984aabaa3252bcecab brcm/brcmfmac43241b5-sdio.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  418098fc9ee888cb4585a641c9333679 brcm/brcmfmac43242a.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  ff610cee869375a2d0c0be6b97b107fb brcm/brcmfmac4329-sdio.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  f27461e1326125e00dd426a3a2782202 brcm/brcmfmac4330-sdio.Prowise-PT301.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  c6908817b83724800bb76b0114eec7a5 brcm/brcmfmac4330-sdio.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  0e5d2b9bfaaf3b6c43077405cfe92632 brcm/brcmfmac4334-sdio.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  8bf2c51176a487dc599ae92472ed508b brcm/brcmfmac43340-sdio.ASUSTeK COMPUTER INC.-TF103CE.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  4824d0bf2e30b9c83bcdd26f2a10697a brcm/brcmfmac43340-sdio.Insyde-VESPA2.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  8bd382215322d7ff9ede2924d3452187 brcm/brcmfmac43340-sdio.bin  identical to linux-firmware cypress/cyfmac43340-sdio.bin (WHENCE Link); LICENCE.broadcom_bcm43xx
  5b4bfe2c5bda0ed81c1f6e382ccc4023 brcm/brcmfmac43340-sdio.meegopad-t08.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  03e9342822abb6d4aa444d33b7b1cef8 brcm/brcmfmac43340-sdio.pov-tab-p1006w-data.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  7fbc9dadaee82c67504b2b707c66f658 brcm/brcmfmac43340-sdio.predia-basic.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  678ed027a1bac420318cd54daa1e1579 brcm/brcmfmac4335-sdio.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  13b101cc43aaab6d492fe19b840dee3e brcm/brcmfmac43362-sdio.ASUSTeK COMPUTER INC.-ME176C.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  40d1aa2dca4c1759257d81ea66d122c6 brcm/brcmfmac43362-sdio.WC121.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  602325b8c42546238307a9bb9d39cb29 brcm/brcmfmac43362-sdio.bin  identical to linux-firmware cypress/cyfmac43362-sdio.bin (WHENCE Link); LICENCE.broadcom_bcm43xx
  c007a7c45f3d0080be4f2c1bc439c053 brcm/brcmfmac43362-sdio.cubietech,cubietruck.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  40d1aa2dca4c1759257d81ea66d122c6 brcm/brcmfmac43362-sdio.kobo,aura.txt  identical to linux-firmware brcm/brcmfmac43362-sdio.WC121.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  40d1aa2dca4c1759257d81ea66d122c6 brcm/brcmfmac43362-sdio.kobo,tolino-shine2hd.txt  identical to linux-firmware brcm/brcmfmac43362-sdio.WC121.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  c007a7c45f3d0080be4f2c1bc439c053 brcm/brcmfmac43362-sdio.lemaker,bananapro.txt  identical to linux-firmware brcm/brcmfmac43362-sdio.cubietech,cubietruck.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  0554efb6792b9688aa04f3dead201794 brcm/brcmfmac4339-sdio.bin  identical to linux-firmware cypress/cyfmac4339-sdio.bin (WHENCE Link); LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.AP6212.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  fdc506b375fd121d595da9fdacb25d55 brcm/brcmfmac43430-sdio.Hampoo-D2D3_Vi8A1.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  2f038b3611fe82414318bc8ed98a535c brcm/brcmfmac43430-sdio.MUR1DX.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.beagle,beaglev-starlight-jh7100-a1.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.AP6212.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.beagle,beaglev-starlight-jh7100-r0.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.AP6212.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  d566c33ab5e8f9fdb1abc63bec9dedae brcm/brcmfmac43430-sdio.bin  identical to linux-firmware cypress/cyfmac43430-sdio.bin (WHENCE Link); LICENCE.broadcom_bcm43xx
  9ff46519b8b8c2cab323322c9d983873 brcm/brcmfmac43430-sdio.clm_blob  identical to linux-firmware cypress/cyfmac43430-sdio.clm_blob (WHENCE Link); LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.friendlyarm,nanopi-neo4.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.AP6212.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.friendlyarm,nanopi-r1.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.AP6212.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  1ab0207fee19b628371fd8f7d4dfd306 brcm/brcmfmac43430-sdio.ilife-S806.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  f823f96f057b31558e70425830858723 brcm/brcmfmac43430-sdio.raspberrypi,3-model-b.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  f823f96f057b31558e70425830858723 brcm/brcmfmac43430-sdio.raspberrypi,model-zero-2-w.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.raspberrypi,3-model-b.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  f823f96f057b31558e70425830858723 brcm/brcmfmac43430-sdio.raspberrypi,model-zero-w.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.raspberrypi,3-model-b.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.sinovoip,bananapi-m64.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.AP6212.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.sinovoip,bpi-m2-plus.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.AP6212.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.sinovoip,bpi-m2-ultra.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.AP6212.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.sinovoip,bpi-m2-zero.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.AP6212.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.sinovoip,bpi-m3.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.AP6212.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  56bd4e83a74244068c07f4045450eff6 brcm/brcmfmac43430-sdio.starfive,visionfive-v1.txt  identical to linux-firmware brcm/brcmfmac43430-sdio.AP6212.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  74e00528d84534ba68fe92c512d11de5 brcm/brcmfmac43430a0-sdio.ONDA-V80 PLUS.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  8c97a1fb04a8c2c0ca0a8ed0814f38ae brcm/brcmfmac43430a0-sdio.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  1eb5661a14ed8c51e6e1d32b0ee1681c brcm/brcmfmac43430a0-sdio.ilife-S806.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  864a714701fc0581f43e5afe155938be brcm/brcmfmac43430a0-sdio.jumper-ezpad-mini3.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.AW-CM256SM.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  33ad683f22dc4bd4b398629821d7a3de brcm/brcmfmac43455-sdio.MINIX-NEO Z83-4.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  b9fda5b86979a3790252019635dc703e brcm/brcmfmac43455-sdio.Radxa-ROCK Pi X.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  70a5c4ab8d7cca57bad76c831c039783 brcm/brcmfmac43455-sdio.Raspberry Pi Foundation-Raspberry Pi 4 Model B.txt  the same bytes as linux-firmware's brcm/brcmfmac43455-sdio.raspberrypi,4-model-b.txt (a second name ROCKNIX gives it); LICENCE.cypress
  70a5c4ab8d7cca57bad76c831c039783 brcm/brcmfmac43455-sdio.Raspberry Pi Foundation-Raspberry Pi Compute Module 4.txt  the same bytes as linux-firmware's brcm/brcmfmac43455-sdio.raspberrypi,4-model-b.txt (a second name ROCKNIX gives it); LICENCE.cypress
  2f99aa1979acf8627757c08f62ce4155 brcm/brcmfmac43455-sdio.acepc-t8.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.beagle,am5729-beagleboneai.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  30bba5424cc300577c6a3f1071832d59 brcm/brcmfmac43455-sdio.bin  identical to linux-firmware cypress/cyfmac43455-sdio.bin (WHENCE Link); LICENCE.broadcom_bcm43xx
  977f6e170e14f2d326e13392a1dc01d5 brcm/brcmfmac43455-sdio.clm_blob  identical to linux-firmware cypress/cyfmac43455-sdio.clm_blob (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.khadas,vim.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,pinebook-pro.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,pinenote-v1.1.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,pinenote-v1.2.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,pinephone-pro.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,quartz64-a.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,quartz64-b.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,rockpro64-v2.0.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,rockpro64-v2.1.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,soquartz-blade.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,soquartz-cm4io.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  5a67b117d487fd9abaa8b0c9c84cd995 brcm/brcmfmac43455-sdio.pine64,soquartz-model-a.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.AW-CM256SM.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  4de8eaa3cb3ca0438a7bad7f11604e49 brcm/brcmfmac43455-sdio.raspberrypi,3-model-a-plus.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.raspberrypi,3-model-b-plus.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  4de8eaa3cb3ca0438a7bad7f11604e49 brcm/brcmfmac43455-sdio.raspberrypi,3-model-b-plus.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  70a5c4ab8d7cca57bad76c831c039783 brcm/brcmfmac43455-sdio.raspberrypi,4-compute-module.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.raspberrypi,4-model-b.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  70a5c4ab8d7cca57bad76c831c039783 brcm/brcmfmac43455-sdio.raspberrypi,4-model-b.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  70a5c4ab8d7cca57bad76c831c039783 brcm/brcmfmac43455-sdio.raspberrypi,5-compute-module.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.raspberrypi,4-model-b.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  70a5c4ab8d7cca57bad76c831c039783 brcm/brcmfmac43455-sdio.raspberrypi,5-model-b.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.raspberrypi,4-model-b.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  70a5c4ab8d7cca57bad76c831c039783 brcm/brcmfmac43455-sdio.raspberrypi,500.txt  identical to linux-firmware brcm/brcmfmac43455-sdio.raspberrypi,4-model-b.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  fa73ea799a4b9d8b4fa29e16148bdd77 brcm/brcmfmac4354-sdio.bin  identical to linux-firmware cypress/cyfmac4354-sdio.bin (WHENCE Link); LICENCE.broadcom_bcm43xx
  b7fb8a88a9fc1bd212ce72e40ca4f954 brcm/brcmfmac4354-sdio.clm_blob  identical to linux-firmware cypress/cyfmac4354-sdio.clm_blob (WHENCE Link); LICENCE.broadcom_bcm43xx
  ef6c7a53539848a52db26ac11b29e146 brcm/brcmfmac4354-sdio.nvidia,p2371-2180.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  f9a647d2dabf5892a8583de5326167f6 brcm/brcmfmac4356-sdio.AP6356S.txt  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  a8e39102e8d4c3374628769d0ce778db brcm/brcmfmac4356-sdio.bin  identical to linux-firmware cypress/cyfmac4356-sdio.bin (WHENCE Link); LICENCE.broadcom_bcm43xx
  4860d1f7b6ecaeefb9abd4f8a39992d6 brcm/brcmfmac4356-sdio.clm_blob  identical to linux-firmware cypress/cyfmac4356-sdio.clm_blob (WHENCE Link); LICENCE.broadcom_bcm43xx
  f9a647d2dabf5892a8583de5326167f6 brcm/brcmfmac4356-sdio.firefly,firefly-rk3399.txt  identical to linux-firmware brcm/brcmfmac4356-sdio.AP6356S.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  f9a647d2dabf5892a8583de5326167f6 brcm/brcmfmac4356-sdio.friendlyarm,nanopc-t4.txt  identical to linux-firmware brcm/brcmfmac4356-sdio.AP6356S.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  f9a647d2dabf5892a8583de5326167f6 brcm/brcmfmac4356-sdio.friendlyarm,nanopi-m4.txt  identical to linux-firmware brcm/brcmfmac4356-sdio.AP6356S.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  f9a647d2dabf5892a8583de5326167f6 brcm/brcmfmac4356-sdio.khadas,vim2.txt  identical to linux-firmware brcm/brcmfmac4356-sdio.AP6356S.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  f9a647d2dabf5892a8583de5326167f6 brcm/brcmfmac4356-sdio.vamrs,rock960.txt  identical to linux-firmware brcm/brcmfmac4356-sdio.AP6356S.txt (WHENCE Link); LICENCE.broadcom_bcm43xx
  6aa4682b48c3c16ac50018457d5adf2d brcm/brcmfmac43569.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  b5199d69b08b6983b219e971527cf6b7 brcm/brcmfmac4373-sdio.bin  identical to linux-firmware cypress/cyfmac4373-sdio.bin (WHENCE Link); LICENCE.broadcom_bcm43xx
  f90fa3ccb4af9b5f29732f1694c77d6a brcm/brcmfmac4373-sdio.clm_blob  identical to linux-firmware cypress/cyfmac4373-sdio.clm_blob (WHENCE Link); LICENCE.broadcom_bcm43xx
  bccbdf86b4d74f36219ff088cf87f607 brcm/brcmfmac4373.bin  identical to linux-firmware file; LICENCE.broadcom_bcm43xx
  2fa6ed98d53d0b5fbcc136d1cf5e9609 carl9170-1.fw  identical to linux-firmware file; GPLv2 or later. Some build scripts use the New BSD (3-clause) licence. See GPL-2 for detai
  fb0a51032c5accf7ecfa853c23c77e3f htc_7010.fw  identical to linux-firmware file; LICENCE.atheros_firmware
  d0e6c63ca68b89f8c874bbff3ebde09a htc_9271.fw  identical to linux-firmware file; LICENCE.atheros_firmware
  9a047587617c9c8732b9c546fb4a0152 mediatek/mt7610u.bin  identical to linux-firmware file; LICENCE.mediatek
  ba969d7c6fe02b2b8d3c744fb2fa1581 mediatek/mt7662u.bin  identical to linux-firmware file; LICENCE.mediatek
  368cdd19ede8b7acbda6632fb4f2b25a mediatek/mt7662u_rom_patch.bin  identical to linux-firmware file; LICENCE.mediatek
  36426a1997896b20167f924008347cc6 meson/vdec/g12a_h264.bin  identical to linux-firmware file; LICENSE.amlogic_vdec
  07fbbf7ff141a281eeee35f490d66ce8 meson/vdec/g12a_hevc_mmu.bin  identical to linux-firmware file; LICENSE.amlogic_vdec
  579d27c421b08c60a51f1759371732a1 meson/vdec/g12a_vp9.bin  identical to linux-firmware file; LICENSE.amlogic_vdec
  696cedb8e76ecc0cda9f9b0d3972c64d mt7601u.bin  identical to linux-firmware mediatek/mt7601u.bin (WHENCE Link); LICENCE.ralink_a_mediatek_company_firmware
  d6d99be5b884d21457340dec16655d1e mt7662.bin  identical to linux-firmware mediatek/mt7662.bin (WHENCE Link); LICENCE.ralink_a_mediatek_company_firmware
  1f2d1c1bbbba58c0468b66ede87ed83c mt7662_rom_patch.bin  identical to linux-firmware mediatek/mt7662_rom_patch.bin (WHENCE Link); LICENCE.ralink_a_mediatek_company_firmware
  7b37a33bbe0f18f342fcd5a9f80bc3fe regulatory.db  wireless-regdb 2026.05.30, identical (as on the H700 and SM8250 cards); wireless-regdb-LICENSE
  1ab34236cbd7434b889a28f5bc3ce530 regulatory.db.p7s  wireless-regdb 2026.05.30, identical; wireless-regdb-LICENSE
  5f36cd9c2b182bc591987c929a3c2773 rt2870.bin  identical to linux-firmware file; LICENCE.ralink-firmware.txt
  bd733372ae21a010bf8a5511d7711c2d rt73.bin  identical to linux-firmware file; LICENCE.ralink-firmware.txt
  167096d687c3a61dba4be98191aab45d rtl_bt/rtl8192ee_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  d91352f436da489e63e13749fb099183 rtl_bt/rtl8192eu_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  8aaacb8316460497459b0db4abd3d565 rtl_bt/rtl8723a_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  57b61c775a51f7a4596950ded7d2d4c0 rtl_bt/rtl8723b_config.bin  identical to linux-firmware rtl_bt/rtl8723bs_config.bin (WHENCE Link); LICENCE.ibt_firmware
  b3363f17ba07a53bfd891fe1b4072e0b rtl_bt/rtl8723b_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  57b61c775a51f7a4596950ded7d2d4c0 rtl_bt/rtl8723bs_config-OBDA0623.bin  identical to linux-firmware rtl_bt/rtl8723bs_config.bin (WHENCE Link); LICENCE.ibt_firmware
  57b61c775a51f7a4596950ded7d2d4c0 rtl_bt/rtl8723bs_config-OBDA8723.bin  identical to linux-firmware rtl_bt/rtl8723bs_config.bin (WHENCE Link); LICENCE.ibt_firmware
  57b61c775a51f7a4596950ded7d2d4c0 rtl_bt/rtl8723bs_config.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  521d95f75577c49eae09b00704c3823d rtl_bt/rtl8723bs_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  2344554fed8337eb1d3e7bc10e9b307f rtl_bt/rtl8723cs_xx_config.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  8506a78d27b8c2aaa02be1a50cae5185 rtl_bt/rtl8723cs_xx_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  6551d1f5c5a9c4ce7ab755d58576e0b0 rtl_bt/rtl8723d_config.bin  identical to linux-firmware rtl_bt/rtl8821c_config.bin (WHENCE Link); LICENCE.ibt_firmware
  e66f08b49eb0c8b79b175298b1be328a rtl_bt/rtl8723d_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  3c1f1152947385057db486e2a235de38 rtl_bt/rtl8723ds_config.bin  NOT TRACED: Realtek Bluetooth firmware that is not in linux-firmware 20260309, from ROCKNIX's packages
  2bf98dd4c855653c207b999e96aa229b rtl_bt/rtl8723ds_fw.bin  NOT TRACED: Realtek Bluetooth firmware that is not in linux-firmware 20260309, from ROCKNIX's packages
  e9a0ee7f815f901ff9347dee4f1e2e11 rtl_bt/rtl8723fu_config.bin  NOT TRACED: Realtek Bluetooth firmware that is not in linux-firmware 20260309, from ROCKNIX's packages
  33d38dbc773a2cf2c2c6c6be2bc306a4 rtl_bt/rtl8723fu_fw.bin  NOT TRACED: Realtek Bluetooth firmware that is not in linux-firmware 20260309, from ROCKNIX's packages
  2111499a33cfed6582b05243167c0c5e rtl_bt/rtl8761a_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  783db7913e59ab81adb335684d5f83fb rtl_bt/rtl8761b_config.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  68132b82523711c49d36ee363cd0b639 rtl_bt/rtl8761b_fw.bin  NOT TRACED: name in linux-firmware 20260309 (LICENCE.rtlwifi_firmware.txt) but other bytes; ROCKNIX ships another version
  49951f548b87ea0258d128195ef6e0cf rtl_bt/rtl8761bu_config.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  e6da3f445524756783270d0da5ddacac rtl_bt/rtl8761bu_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  65bbb0c01ddc2005081e1e2684331f6a rtl_bt/rtl8761cu_config.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  02fe0df6ff0736a96321e24d1f306539 rtl_bt/rtl8761cu_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  87e2acd0a8da7f8c8f8f5ed1597dce0c rtl_bt/rtl8812ae_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  6551d1f5c5a9c4ce7ab755d58576e0b0 rtl_bt/rtl8821a_config.bin  identical to linux-firmware rtl_bt/rtl8821c_config.bin (WHENCE Link); LICENCE.ibt_firmware
  60fc67b39bfcb6311c369a3f7531cd28 rtl_bt/rtl8821a_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  6551d1f5c5a9c4ce7ab755d58576e0b0 rtl_bt/rtl8821c_config.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  bf9ff07d4c7de0e241e7e3024288aa80 rtl_bt/rtl8821c_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  783db7913e59ab81adb335684d5f83fb rtl_bt/rtl8821cs_config.bin  identical to linux-firmware rtl_bt/rtl8761b_config.bin (WHENCE Link); LICENCE.ibt_firmware
  10f34596581e022af3658731b4af853f rtl_bt/rtl8821cs_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  3eb96095411a33c339b3da2a7e9d94f3 rtl_bt/rtl8822b_config.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  547896dc612f5e4a704e4bff8dfb908b rtl_bt/rtl8822b_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  75dfc3a249209f23913545a25b044a23 rtl_bt/rtl8822cs_config.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  343194432d63d2d29a8988c990eb1412 rtl_bt/rtl8822cs_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  49951f548b87ea0258d128195ef6e0cf rtl_bt/rtl8822cu_config.bin  identical to linux-firmware rtl_bt/rtl8761bu_config.bin (WHENCE Link); LICENCE.ibt_firmware
  548a63bc8b4c80bbb06dba9e8e790328 rtl_bt/rtl8822cu_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  49951f548b87ea0258d128195ef6e0cf rtl_bt/rtl8851bu_config.bin  identical to linux-firmware rtl_bt/rtl8761bu_config.bin (WHENCE Link); LICENCE.ibt_firmware
  b6e0ab33d70199e0d0f71391318e2093 rtl_bt/rtl8851bu_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  49951f548b87ea0258d128195ef6e0cf rtl_bt/rtl8852au_config.bin  identical to linux-firmware rtl_bt/rtl8761bu_config.bin (WHENCE Link); LICENCE.ibt_firmware
  5fa9abb09a6c35d11a57adcca9a2fc7b rtl_bt/rtl8852au_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  07cc2e1115a52e7f43624f1a7167be9d rtl_bt/rtl8852bs_config.bin  NOT TRACED: Realtek Bluetooth firmware that is not in linux-firmware 20260309, from ROCKNIX's packages
  d2d79756b000d861dec32aa5804b5089 rtl_bt/rtl8852bs_fw.bin  NOT TRACED: Realtek Bluetooth firmware that is not in linux-firmware 20260309, from ROCKNIX's packages
  49951f548b87ea0258d128195ef6e0cf rtl_bt/rtl8852btu_config.bin  identical to linux-firmware rtl_bt/rtl8761bu_config.bin (WHENCE Link); LICENCE.ibt_firmware
  f0a59e994165f1864a2d2665922d51e5 rtl_bt/rtl8852btu_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  49951f548b87ea0258d128195ef6e0cf rtl_bt/rtl8852bu_config.bin  identical to linux-firmware rtl_bt/rtl8761bu_config.bin (WHENCE Link); LICENCE.ibt_firmware
  7445c08fbf38c3022fe35becd8c980b8 rtl_bt/rtl8852bu_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  49951f548b87ea0258d128195ef6e0cf rtl_bt/rtl8852cu_config.bin  identical to linux-firmware rtl_bt/rtl8761bu_config.bin (WHENCE Link); LICENCE.ibt_firmware
  c450dcda57df4a78f9ee7f338a844180 rtl_bt/rtl8852cu_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  696437751a0bd73313c0c10483b7f86a rtl_bt/rtl8852cu_fw_v2.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  49951f548b87ea0258d128195ef6e0cf rtl_bt/rtl8922au_config.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  98acede261f6d45db5012bb6f49b5460 rtl_bt/rtl8922au_fw.bin  identical to linux-firmware file; LICENCE.ibt_firmware
  466e72e2e3b01015cbd666424586f199 rtl_nic/rtl8105e-1.fw  identical to linux-firmware file; LICENSE.r8169
  f1c44f8be0f4d9381a1060a6b3bf72a6 rtl_nic/rtl8106e-1.fw  identical to linux-firmware file; LICENSE.r8169
  bbaa36b6f3c80b459bb8ff0958b64c23 rtl_nic/rtl8106e-2.fw  identical to linux-firmware file; LICENSE.r8169
  214bc766e7571392a80d0244e8665b37 rtl_nic/rtl8107e-1.fw  identical to linux-firmware file; LICENSE.r8169
  46940e91cf855840cdb0630f8d8837d2 rtl_nic/rtl8107e-2.fw  identical to linux-firmware file; LICENSE.r8169
  0a38939662756932e20fbebc37897d14 rtl_nic/rtl8125a-3.fw  identical to linux-firmware file; LICENSE.r8169
  c01dbdf49123d6f1e1c432921bc9d5dc rtl_nic/rtl8125b-1.fw  identical to linux-firmware file; LICENSE.r8169
  3bd3db7cf923ea77932d6f348f752d46 rtl_nic/rtl8125b-2.fw  identical to linux-firmware file; LICENSE.r8169
  75f8aad546b9c129868956ab9e86ca68 rtl_nic/rtl8125bp-2.fw  identical to linux-firmware file; LICENSE.r8169
  1bc5fbc2c3668c156c3e855562ed7f44 rtl_nic/rtl8125cp-1.fw  identical to linux-firmware file; LICENSE.r8169
  c57ba5972be4ba5e61a087918f385ff0 rtl_nic/rtl8125d-1.fw  identical to linux-firmware file; LICENSE.r8169
  84775919f604ba22636beb432b5353b8 rtl_nic/rtl8125d-2.fw  identical to linux-firmware file; LICENSE.r8169
  8cd488b82c805260283dea6d70efbb37 rtl_nic/rtl8125k-1.fw  identical to linux-firmware file; LICENSE.r8169
  49b514ec876d071d0ff1af130e6d3e88 rtl_nic/rtl8126a-2.fw  identical to linux-firmware file; LICENSE.r8169
  95e667970330d87623429e707ab06453 rtl_nic/rtl8126a-3.fw  identical to linux-firmware file; LICENSE.r8169
  8fc5554c7fd54f4ba71867ca5561ce6e rtl_nic/rtl8127a-1.fw  identical to linux-firmware file; LICENSE.r8169
  24e5c93dc03c61ab21aaa06eb2feee5b rtl_nic/rtl8153a-2.fw  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  dc30caf7bd9a322758ceeb6efef71a73 rtl_nic/rtl8153a-3.fw  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  0ca34576a074e303c3d1ba72dae2d92d rtl_nic/rtl8153a-4.fw  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  67e52f39a687a4305995718aa106b7ec rtl_nic/rtl8153b-2.fw  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  f81d64851f52de6d1fb72a070e0f90bb rtl_nic/rtl8153c-1.fw  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  2102de5d911dc49a75c130ba390e5977 rtl_nic/rtl8156a-2.fw  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  feecab2d008ee24455bbe27bb2a65767 rtl_nic/rtl8156b-2.fw  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  500d938ce15d1b0aff8d394aefb7a812 rtl_nic/rtl8168d-1.fw  identical to linux-firmware file; LICENSE.r8169
  b14a1a124c2d58fd346ed25ffbbe2959 rtl_nic/rtl8168d-2.fw  identical to linux-firmware file; LICENSE.r8169
  86f64bfd2ca97ddf67a4ecbdd5275fa2 rtl_nic/rtl8168e-1.fw  identical to linux-firmware file; LICENSE.r8169
  6f5c444506137276f405dff374f2e910 rtl_nic/rtl8168e-2.fw  identical to linux-firmware file; LICENSE.r8169
  46843793bae7d5b81317a0a52ba5eccb rtl_nic/rtl8168e-3.fw  identical to linux-firmware file; LICENSE.r8169
  624831688e25aa47fa84c30c045fcae3 rtl_nic/rtl8168f-1.fw  identical to linux-firmware file; LICENSE.r8169
  8a1ff9b90bcdf6e9df4ef02aa89dcccd rtl_nic/rtl8168f-2.fw  identical to linux-firmware file; LICENSE.r8169
  a27cd7a4fc485b655c3fcdbd32ef786c rtl_nic/rtl8168fp-3.fw  identical to linux-firmware file; LICENSE.r8169
  d6e4d404b03668aafdbce03e9b6f5341 rtl_nic/rtl8168g-1.fw  identical to linux-firmware file; LICENSE.r8169
  fd88afedf0330844015ad58f09f9b386 rtl_nic/rtl8168g-2.fw  identical to linux-firmware file; LICENSE.r8169
  09483ae9732a8581c472284b53f82cda rtl_nic/rtl8168g-3.fw  identical to linux-firmware file; LICENSE.r8169
  dcd7adbf77cf62df09f64cde73b664d9 rtl_nic/rtl8168h-1.fw  identical to linux-firmware file; LICENSE.r8169
  11b4899d299ed119ba41099739145f69 rtl_nic/rtl8168h-2.fw  identical to linux-firmware file; LICENSE.r8169
  58334d8e3017e292d4ba17920c150b88 rtl_nic/rtl8402-1.fw  identical to linux-firmware file; LICENSE.r8169
  1d21c49e48b57cab26ddd001bc4eb0de rtl_nic/rtl8411-1.fw  identical to linux-firmware file; LICENSE.r8169
  15fe6fb2ad8a0df590611d086c4f8c8c rtl_nic/rtl8411-2.fw  identical to linux-firmware file; LICENSE.r8169
  50debbe0eaee7a1da24cac9b369bab56 rtl_nic/rtl9151a-1.fw  identical to linux-firmware file; LICENSE.r8169
  3aed09e6385aff0f3809a3949f6c3b64 rtlwifi/rtl8188efw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  b66178b87ed8f917997b9cf38fc0cfd3 rtlwifi/rtl8188eufw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  62e540665cf25e682864c1ef67b893ba rtlwifi/rtl8188fufw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  fd118c183ad9e11060a6e575b472280e rtlwifi/rtl8192cfw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  660d5d7e1e1fd985261fb20b2ac283f6 rtlwifi/rtl8192cfwU.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  22d58a35a1e4d75367a8cdb64eee45c3 rtlwifi/rtl8192cfwU_B.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  943e9b714a926e630b8152d7aad91d2e rtlwifi/rtl8192cufw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  56ee8866b9ee0e96eda34bce2eb3dd02 rtlwifi/rtl8192cufw_A.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  922cb0b28827d77ec43ad29ce9f3f3ca rtlwifi/rtl8192cufw_B.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  4bd61757aa1b7bf1a73b1796384b4f36 rtlwifi/rtl8192cufw_TMSC.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  01c66e417fc8dbed78a9edefcd802d9b rtlwifi/rtl8192defw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  18eb8791673dcc38b268ad19c92ba6a2 rtlwifi/rtl8192dufw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  ed1fd62b6566a0fed44d46ba1b825a5b rtlwifi/rtl8192eefw.bin  identical to linux-firmware rtlwifi/rtl8192eu_nic.bin (WHENCE Link); LICENCE.rtlwifi_firmware.txt
  b33f15ef1a86d0a99b3a64b3caf6fc92 rtlwifi/rtl8192eu_ap_wowlan.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  ed1fd62b6566a0fed44d46ba1b825a5b rtlwifi/rtl8192eu_nic.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  3558003e8cb24da617509e09b012a7ae rtlwifi/rtl8192eu_wowlan.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  5f4a9c5a58621e82c198fb16615d3563 rtlwifi/rtl8192fufw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  2faf6915a0bbb5d15d3bd43618f5347c rtlwifi/rtl8192sefw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  731a5cd848a10f727d7968eb0d13cca7 rtlwifi/rtl8710bufw_SMIC.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  8d52a29092895cd06122518890ecc479 rtlwifi/rtl8710bufw_UMC.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  200fd952db3cc9259b1fd05e3e51966f rtlwifi/rtl8712u.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  e1e93bf79ff5c01141f314843ca7e695 rtlwifi/rtl8723aufw_A.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  adcba90301631f1d6c27977490953add rtlwifi/rtl8723aufw_B.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  229ecd17913262139ec3c618864a65cc rtlwifi/rtl8723aufw_B_NoBT.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  bc828ddc6cd700abb79a8df5794bcf04 rtlwifi/rtl8723befw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  1850c1308fbcd95e9f6a7f58ede1e35f rtlwifi/rtl8723befw_36.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  30240ec2230370617b3704cc2ed5895d rtlwifi/rtl8723bs_ap_wowlan.bin  identical to linux-firmware rtlwifi/rtl8723bu_ap_wowlan.bin (WHENCE Link); LICENCE.rtlwifi_firmware.txt
  f51dc85272c72353fb69f0d379dbd15a rtlwifi/rtl8723bs_bt.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  67d4aad0db155a701610b156473a75fd rtlwifi/rtl8723bs_nic.bin  identical to linux-firmware rtlwifi/rtl8723bu_nic.bin (WHENCE Link); LICENCE.rtlwifi_firmware.txt
  2bf10334c296ec239378933b4e6908b6 rtlwifi/rtl8723bs_wowlan.bin  identical to linux-firmware rtlwifi/rtl8723bu_wowlan.bin (WHENCE Link); LICENCE.rtlwifi_firmware.txt
  30240ec2230370617b3704cc2ed5895d rtlwifi/rtl8723bu_ap_wowlan.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  67d4aad0db155a701610b156473a75fd rtlwifi/rtl8723bu_nic.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  2bf10334c296ec239378933b4e6908b6 rtlwifi/rtl8723bu_wowlan.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  984e05af120b2e080e4bd5ac65dcf4f9 rtlwifi/rtl8723defw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  69ccaffbe94cc0ef1b89c25290e19b2e rtlwifi/rtl8723fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  ce50dfe07dbb1bfe9e14bdb315a4b28a rtlwifi/rtl8723fw_B.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  013a9205adde60d777da0d93c2ecc4b2 rtlwifi/rtl8812aefw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  c2fa468f9ac8d873dfe8fbb02685c21e rtlwifi/rtl8812aefw_wowlan.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  61d4e7322431fafa20070ee451663f8f rtlwifi/rtl8821aefw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  a279d53d25e207a90e5053809d182af8 rtlwifi/rtl8821aefw_29.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  eb7a115cfa5ac2bf44f7124e54b4334d rtlwifi/rtl8821aefw_wowlan.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  1d57eef7f5b3baf2ea5de3b9d87d2c66 rtlwifi/rtl8822befw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  00b93a4bd3033ccc517cbeeb03cc41c5 rtw88/README  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  e82e5fc8c2f549c1480bce0d95d7f525 rtw88/rtw8703b_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  f736a3f057a64e05ae01706b2af027e0 rtw88/rtw8703b_wow_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  ea7a621393871e579bcca1b9539e9639 rtw88/rtw8723d_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  bb4bb6a5f26819c180a177f3abdb414d rtw88/rtw8812a_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  42b8d181d0aaa7946ebec6d0bd5f068d rtw88/rtw8814a_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  b5fdc50011a441dba8fc140fbbedc342 rtw88/rtw8821a_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  2638a40042a27e98df2f7929c8eee760 rtw88/rtw8821c_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  e155ac20463300491d893ae2906915d8 rtw88/rtw8822b_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  4045f5e9b31b7a8a3823786620c4d1fc rtw88/rtw8822c_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  130ecf9de2ff6917401370f80ce63fb6 rtw88/rtw8822c_wow_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  288ab8378f14f517b7a3ce881e27c6da rtw89/rtw8851b_fw-1.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  bf160e0f84a550603d3cb97968c01e94 rtw89/rtw8851b_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  db762be8a1fa2520d2806e95d4d0c0e0 rtw89/rtw8852a_fw-1.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  249ddc0d8c4aeddfae538d5489f67b34 rtw89/rtw8852a_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  7deadc27c70ae4f079f4047157ed56af rtw89/rtw8852b_fw-1.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  be47096b6c980701817578acec058524 rtw89/rtw8852b_fw-2.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  a3317e9646c8e669c7c2d1fc19a6109d rtw89/rtw8852b_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  1b95247f13c2ce129c06ff9100f3e5bf rtw89/rtw8852bt_fw-1.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  c0cfc7a6b026921225e1446de1fbf8b1 rtw89/rtw8852bt_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  b78bf3918e31e093e581a0fd6d5ed8bd rtw89/rtw8852c_fw-1.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  ac5aa419596e9718de6f77df08f8c333 rtw89/rtw8852c_fw-2.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  db00433f1d82eea5ad53f76e7247f6b5 rtw89/rtw8852c_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  6c646f1ce5685cf74805cdc2ba564df4 rtw89/rtw8922a_fw-1.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  2f44d7b32b2eb7f377d5e1dec73967f0 rtw89/rtw8922a_fw-2.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  b368dce92a6b3b23baeea7dfedf61673 rtw89/rtw8922a_fw-3.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  ae7b4568f449175399045c334e7cad8f rtw89/rtw8922a_fw-4.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  d21c3e438dd8d02ee7e072023005c194 rtw89/rtw8922a_fw.bin  identical to linux-firmware file; LICENCE.rtlwifi_firmware.txt
  5a2b1592576b4c85e684229daca0aad4 vntwusb.fw  identical to linux-firmware file; LICENCE.via_vt6656
