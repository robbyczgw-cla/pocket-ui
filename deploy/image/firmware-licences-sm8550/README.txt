Firmware inside SYSTEM (usr/lib/kernel-overlays/base/lib/firmware) and its terms: SM8550 images
==============================================================================================

ROCKNIX 20261001 puts the files below into SYSTEM for the SM8550 devices. All are unchanged.

linux-firmware 20260309 (https://cdn.kernel.org/pub/linux/kernel/firmware/linux-firmware-20260309.tar.gz,
sha256 28fb1b57fa9531edbd001a2462068267f10622e613abd40f952344b3f1fc5a16, ROCKNIX's kernel-firmware package and copies in
extra-firmware), md5 compared on 2026-10-31:

  md5                              file                                  terms (WHENCE)
  b557c45d9344cd797dae42f08d1749ac ath12k/WCN7850/hw2.0/amss.bin         LICENSE.QualcommAtheros_ath10k
  7035ec45af1f3b145a8dc6286d2330a6 ath12k/WCN7850/hw2.0/m3.bin           LICENSE.QualcommAtheros_ath10k
  9f728e4a31c495d911e3c429b2152aa2 qca/hmtbtfw20.tlv                     LICENSE.qcom, qca-NOTICE.txt
  f54d26086c3a561d7e4d77fb160639d3 qca/hmtnv20.b10f                      LICENSE.qcom, qca-NOTICE.txt
  f54d26086c3a561d7e4d77fb160639d3 qca/hmtnv20.b112                      LICENSE.qcom, qca-NOTICE.txt
  c04972ef999748e51cd6798a4505dc6d qca/hmtnv20.bin                       LICENSE.qcom, qca-NOTICE.txt
  0211fdf6aa6f523e9d392f31ffbf10a1 qcom/a740_sqe.fw                      LICENSE.qcom, qcom-NOTICE.txt
  e7b1bad8c1b24f941e563f2c50ff03bc qcom/gmu_gen70200.bin                 LICENSE.qcom, qcom-NOTICE.txt
  676c4238e6c368e7fa0ef85fa0e9088b qcom/sm8550/a740_zap.mbn              LICENSE.qcom, qcom-NOTICE.txt
  0ca34576a074e303c3d1ba72dae2d92d rtl_nic/rtl8153a-4.fw                 LICENCE.rtlwifi_firmware.txt

wireless-regdb 2026.05.30 (https://www.kernel.org/pub/software/network/wireless-regdb/wireless-regdb-2026.05.30.tar.xz,
sha256 8a27bfc081bafed8c24dd70fab0d96f098e5a0bfcd08d3da672595f225ab8993)

  7b37a33bbe0f18f342fcd5a9f80bc3fe regulatory.db                         wireless-regdb-LICENSE (ISC-style)
  1ab34236cbd7434b889a28f5bc3ce530 regulatory.db.p7s                     wireless-regdb-LICENSE

ROCKNIX's extra-firmware (https://github.com/ROCKNIX/extra-firmware, commit 88b363e67d4f730feb2c3124724d26dfaa88ce76).
The repository has no licence file and states no terms; ROCKNIX's package says PKG_LICENSE="proprietary". WE HAVE NO
STATED PERMISSION TO PASS THESE ON. They are passed on unchanged, as ROCKNIX does, because the handhelds need them: the
device trees load adsp/cdsp by these names, and sound, the battery reading and USB-C run on them. We remove them on request
of whoever holds the rights.

  8fb2b605f036f06706e7ee786773e797 ath12k/WCN7850/hw2.0/board-2.bin      not linux-firmware's file of that name
  e84783a5bcd720fa2634dd5f4192b046 ath12k/WCN7850/hw2.0/regdb.bin        not in linux-firmware
  4d2dae9a8187b728939e9c79fa68012b qcom/vpu/vpu30_p4.mbn                 not in linux-firmware under this name
  11b49c68a400564b704c6ef17a0e6c0a renesas_usb_fw.mem                    Renesas USB 3 controller; not in linux-firmware
  5e2ffc3e3fa9fb17eeeedf7b2077f196 qcom/sm8550/AYN-Odin2-tplg.bin        sound topology, also linked as AYN-Thor-tplg.bin
  bcf8c4d7837307623d5e7b948b4f87e2 qcom/sm8550/SM8550-APS-tplg.bin       sound topology
  74895e4eeafde1c7dff3604635501b01 qcom/sm8550/ayn/cdsp.mbn
  7f03e78ce54a7b2e4892bea2fb6b9558 qcom/sm8550/ayn/cdsp_dtb.mbn
  824b9e96936bea096645a1309111ae2e qcom/sm8550/ayn/odin2/adsp.mbn
  c345a53cb4640adc54cbfd0173917d25 qcom/sm8550/ayn/odin2/adsp_dtb.mbn
  5d262bb7661402a8d7ca9724bbb45d46 qcom/sm8550/ayn/odin2/adspr.jsn
  cddb328503f04c634cc3251b89dad765 qcom/sm8550/ayn/odin2/adsps.jsn
  a43d66d0549e33cf421e25be06260ce0 qcom/sm8550/ayn/odin2/adspua.jsn
  0407fff899fca71c033f70da01daf336 qcom/sm8550/ayn/odin2/aw883xx_acf.bin
  0d755b18c24337ebca8313b85f8e8a6c qcom/sm8550/ayn/odin2/battmgr.jsn
  1051ea7a24f7d65e43bd499e1876272f qcom/sm8550/ayn/odin2mini/adsp.mbn
  2b05f32580f10c55dc30cfeb4f80a317 qcom/sm8550/ayn/odin2mini/adsp_dtb.mbn
  5d262bb7661402a8d7ca9724bbb45d46 qcom/sm8550/ayn/odin2mini/adspr.jsn
  cddb328503f04c634cc3251b89dad765 qcom/sm8550/ayn/odin2mini/adsps.jsn
  a43d66d0549e33cf421e25be06260ce0 qcom/sm8550/ayn/odin2mini/adspua.jsn
  f92405fffa06f3f433f8282df82be124 qcom/sm8550/ayn/odin2mini/aw883xx_acf.bin
  0d755b18c24337ebca8313b85f8e8a6c qcom/sm8550/ayn/odin2mini/battmgr.jsn
  826ab9328d092b7b33af8b3d0769a140 qcom/sm8550/ayn/odin2portal/adsp.mbn
  4ce6afda37d23eff3c94fa33faa83809 qcom/sm8550/ayn/odin2portal/adsp_dtb.mbn
  5d262bb7661402a8d7ca9724bbb45d46 qcom/sm8550/ayn/odin2portal/adspr.jsn
  cddb328503f04c634cc3251b89dad765 qcom/sm8550/ayn/odin2portal/adsps.jsn
  a43d66d0549e33cf421e25be06260ce0 qcom/sm8550/ayn/odin2portal/adspua.jsn
  0407fff899fca71c033f70da01daf336 qcom/sm8550/ayn/odin2portal/aw883xx_acf.bin
  0d755b18c24337ebca8313b85f8e8a6c qcom/sm8550/ayn/odin2portal/battmgr.jsn

The Retroid Pocket 6 and Nova load the qcom/sm8550/ayn/odin2/ files (ROCKNIX's device trees).

Not on this card: firmware of the AYANEO Pocket family (qcom/sm8550/ayaneo) and the AYN Thor (qcom/sm8550/ayn/thor),
from the same repository. No device this image is for uses them, and this image's SYSTEM is repacked without them.
