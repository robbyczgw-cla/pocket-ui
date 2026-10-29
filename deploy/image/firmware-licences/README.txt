Firmware inside SYSTEM (usr/lib/kernel-overlays/base/lib/firmware) and its terms
==================================================================================

ROCKNIX 20261001 puts these files into SYSTEM for the SM8250 devices. Each one was compared with its
upstream file (md5 below) on 2026-10-28: all are unchanged. linux-firmware's own index, WHENCE, names the
terms of each file; the texts it points to are in this folder, as linux-firmware ships them.

linux-firmware 20260309 (https://cdn.kernel.org/pub/linux/kernel/firmware/linux-firmware-20260309.tar.gz,
sha256 28fb1b57fa9531edbd001a2462068267f10622e613abd40f952344b3f1fc5a16, ROCKNIX's kernel-firmware package)

  md5                              file                              terms (WHENCE)
  a2b9504e3d1cb8686ae7fcca21a11e1a qcom/a650_gmu.bin                 LICENSE.qcom, qcom-NOTICE.txt
  dc02e77a04b97b985281d36ab159c130 qcom/a650_sqe.fw                  LICENSE.qcom, qcom-NOTICE.txt
  22d41ed3f78026773951988f04d34205 qcom/sm8250/a650_zap.mbn          LICENSE.qcom, qcom-NOTICE.txt
  9ead68f99d30013c6747216ae8cc74f7 qcom/sm8250/adsp.mbn              LICENSE.qcom, qcom-NOTICE.txt
  b60a1a6bfab3bc8c995b0aa5080c9b71 qcom/sm8250/cdsp.mbn              LICENSE.qcom, qcom-NOTICE.txt
  18d968997139fd3e5bd701dbf7c69eff qcom/sm8250/slpi.mbn              LICENSE.qcom, qcom-NOTICE.txt
  2439d6872b39e9c1282c572b266665f0 qcom/sm8250/slpir.jsn             (upstream qcom/sm8250/Thundercomm/RB5/,
  0039483ebd20685728df72c12d13fee4 qcom/sm8250/slpius.jsn             moved by ROCKNIX's package)
  6bbf69e973deb78a60b7deaccc573683 qcom/vpu-1.0/venus.mbn            LICENSE.qcom, qcom-NOTICE.txt
                                                                     (upstream qcom/vpu/vpu20_p4.mbn)
  682f7ca2e0b3ea16644c3585772a2cba ath11k/QCA6390/hw2.0/amss.bin     LICENSE.QualcommAtheros_ath10k, ath11k-QCA6390-hw2.0-Notice.txt
  99ace0a453a7c12e3db1c3355fc34e39 ath11k/QCA6390/hw2.0/board-2.bin  LICENSE.QualcommAtheros_ath10k, ath11k-QCA6390-hw2.0-Notice.txt
  fd4aa4a58f33854a751ec7d14d77ce91 ath11k/QCA6390/hw2.0/m3.bin       LICENSE.QualcommAtheros_ath10k, ath11k-QCA6390-hw2.0-Notice.txt
  109ce4287557f1789f2e0ed41e08b5ce qca/htbtfw20.tlv                  LICENSE.qcom, qca-NOTICE.txt
  74ac04b7aa5e8d988e4813f6d9a20094 qca/htnv20.bin                    LICENSE.qcom, qca-NOTICE.txt
  0ca34576a074e303c3d1ba72dae2d92d rtl_nic/rtl8153a-4.fw             LICENCE.rtlwifi_firmware.txt

  In linux-firmware the NOTICE files are qcom/NOTICE.txt, qca/NOTICE.txt and ath11k/QCA6390/hw2.0/Notice.txt;
  here they carry the folder in their name.

wireless-regdb 2026.05.30 (https://www.kernel.org/pub/software/network/wireless-regdb/wireless-regdb-2026.05.30.tar.xz,
sha256 8a27bfc081bafed8c24dd70fab0d96f098e5a0bfcd08d3da672595f225ab8993)

  7b37a33bbe0f18f342fcd5a9f80bc3fe regulatory.db                     wireless-regdb-LICENSE (ISC-style)
  1ab34236cbd7434b889a28f5bc3ce530 regulatory.db.p7s                 wireless-regdb-LICENSE

All of these are "Redistributable" in WHENCE. LICENSE.qcom allows passing the files on in binary form, for
use with Qualcomm chipsets (the Mini V2's SM8250 is one), with the licence text included (this folder) and
the notices kept.

Not on this card: ROCKNIX's SYSTEM also holds firmware for the AYN Thor (qcom/sm8250/ayn/) and the Mangmi
Pocket Max (qcom/sm8250/mangmi/) from ROCKNIX's extra-firmware repository, which states no licence. The
Mini V2 does not use it, and this image's SYSTEM is repacked without it.
