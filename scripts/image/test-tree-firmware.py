#!/usr/bin/env python3
"""A valid speaker label must not become a missing firmware error."""
import importlib.util
from pathlib import Path
import unittest
spec = importlib.util.spec_from_file_location('firmware', Path(__file__).with_name('tree-firmware.py'))
firmware = importlib.util.module_from_spec(spec)
spec.loader.exec_module(firmware)


class FirmwarePaths(unittest.TestCase):
    def test_classic_speaker_labels_are_not_firmware(self):
        dts = '''speaker {\n firmware-name = "qcom/amp.bin";\n sound-name-prefix = "Left";\n};
other {\n firmware-name = "qcom/amp.bin";\n sound-name-prefix = "Right";\n};'''
        self.assertEqual(firmware.firmware_names(dts), ['qcom/amp.bin'])

    def test_multiline_arrays_and_nul_lists_keep_every_blob(self):
        dts = r'''dsp {
 firmware-name = "qcom/adsp.mbn",
                 "qcom/wpss.mbn\0qcom/zap.mbn";
 model = "Retroid Pocket Classic Rev Q";
};'''
        self.assertEqual(firmware.firmware_names(dts), ['qcom/adsp.mbn', 'qcom/wpss.mbn', 'qcom/zap.mbn'])


if __name__ == '__main__':
    unittest.main()
