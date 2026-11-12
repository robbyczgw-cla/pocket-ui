Firmware in the SM4450 Classic Rev Q beta
=========================================

ROCKNIX 20261101 firmware is unchanged except the unused Anbernic RG55G1 folder is removed.
The Classic tree loads qcom/sm4450/RPClassic/{adsp.mbn,wpss.mbn,aw88166_acf.bin},
qcom/sm4450/a613_zap.mbn, qcom/a630_sqe.fw and WCN3990 firmware. Its sound topology
is qcom/sm8450/RetroidPocketClassic-tplg.bin.

linux-firmware terms are supplied next to this file (LICENSE.qcom, qcom-NOTICE.txt,
LICENSE.QualcommAtheros_ath10k, qca-NOTICE.txt, LICENCE.rtlwifi_firmware.txt).
wireless-regdb's terms are wireless-regdb-LICENSE.

The device-specific firmware comes from ROCKNIX's extra-firmware repository,
https://github.com/ROCKNIX/extra-firmware (ROCKNIX 20261101 recipe). That repository
states no redistribution licence; the ROCKNIX recipe calls it proprietary.
There is no stated permission to redistribute those blobs. This private test image
keeps the Classic firmware unchanged as upstream does because sound, battery and
Wi-Fi need it. This is an unresolved licence question before any public release.
ROCKNIX's unlicensed ABL binary and all its flashing payloads are removed.

Inventory of every regular firmware file in the stripped pinned SYSTEM (MD5, path):
  7f52ba16700d317024b8d5fb5783f7e8  ath10k/WCN3990/hw1.0/board-2.bin  kernel-firmware / extra-firmware; Qualcomm terms supplied; vendor provenance unverified
  d16e3444f68ee48c548a891b9f9279e1  ath10k/WCN3990/hw1.0/firmware-5.bin  kernel-firmware / extra-firmware; Qualcomm terms supplied; vendor provenance unverified
  259b4f9e4aef57a5051f27a201653262  ath10k/WCN3990/hw1.0/wlanmdsp.mbn  kernel-firmware / extra-firmware; Qualcomm terms supplied; vendor provenance unverified
  2bf65cf559aa9822c27f06b079d3e7b3  qca/cmbtfw13.tlv  LICENSE.qcom and qca-NOTICE.txt
  934533b412d2d235b249b7b86ce0f5fd  qca/cmbtfw13.ver  LICENSE.qcom and qca-NOTICE.txt
  2e986d293807ca5de84f2b8b35169254  qca/cmnv13.bin  LICENSE.qcom and qca-NOTICE.txt
  1971a90f534222655762897a66fca350  qca/cmnv13s.bin  LICENSE.qcom and qca-NOTICE.txt
  2e986d293807ca5de84f2b8b35169254  qca/cmnv13t.bin  LICENSE.qcom and qca-NOTICE.txt
  9f2540d789d9fd4699a566d97fcabded  qcom/a630_sqe.fw  LICENSE.qcom and qcom-NOTICE.txt
  b0faf1f19cddbb4f5fed39b42006461a  qcom/sm4450/RPClassic/adsp.mbn  extra-firmware; no stated redistribution licence
  f92405fffa06f3f433f8282df82be124  qcom/sm4450/RPClassic/aw88166_acf.bin  extra-firmware; no stated redistribution licence
  1e706d6bda379182fd5149f095125b13  qcom/sm4450/RPClassic/wpss.mbn  extra-firmware; no stated redistribution licence
  6e12017afa5e74a6198ec38e11be6903  qcom/sm4450/a613_zap.mbn  extra-firmware; no stated redistribution licence
  e3bda93dd455fac6d339af21a8f327ab  qcom/sm8450/AnbernicRG55G1-tplg.bin  ROCKNIX SM4450 topology sources, BSD-3-Clause (TOPOLOGY-BSD-3-CLAUSE.txt)
  e3bda93dd455fac6d339af21a8f327ab  qcom/sm8450/RetroidPocketClassic-tplg.bin  ROCKNIX SM4450 topology sources, BSD-3-Clause (TOPOLOGY-BSD-3-CLAUSE.txt)
  7b37a33bbe0f18f342fcd5a9f80bc3fe  regulatory.db  wireless-regdb-LICENSE
  1ab34236cbd7434b889a28f5bc3ce530  regulatory.db.p7s  wireless-regdb-LICENSE

Topology sources: https://github.com/ROCKNIX/distribution/tree/20261101/projects/ROCKNIX/devices/SM4450/topology
The two topology files are byte-identical; the unused Anbernic sound topology remains as in upstream.
