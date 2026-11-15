"""Cross-platform install identity checks; no installed app or registry is changed."""

import json
from pathlib import Path
import plistlib
import unittest
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]


class ForkInstallContract(unittest.TestCase):
    def test_macos_bundle_is_grid_and_does_not_replace_upstream(self):
        info = plistlib.loads((ROOT / "packaging/macos/Info.plist.in").read_bytes())
        self.assertEqual(info["CFBundleDisplayName"], "Grid")
        self.assertEqual(info["CFBundleExecutable"], "Grid")
        self.assertEqual(info["CFBundleIdentifier"], "io.github.jeongeee.grid")
        self.assertTrue(all(t["LSHandlerRank"] == "Alternate" for t in info["CFBundleDocumentTypes"]))

    def test_windows_install_has_its_own_identity_and_payloads(self):
        policy = json.loads((ROOT / "packaging/windows/install-policy.json").read_text())
        self.assertEqual(policy["id"], "grid")
        self.assertEqual(policy["name"], "Grid")
        self.assertNotEqual(policy["upgradeCode"], "5511327B-F305-41BE-92D0-EB84944E7C4D")
        self.assertEqual(sorted(policy["payloads"]), ["grid-cli.exe", "grid.exe"])
        for row in policy["registry"]:
            self.assertNotIn("GridCraft", row[1])
            self.assertNotIn("App Paths\\gridcraft.exe", row[1])

    def test_windows_components_do_not_reuse_upstream_guids(self):
        root = ET.parse(ROOT / "packaging/windows/gridcraft.wxs").getroot()
        guids = {node.attrib["Guid"] for node in root.iter() if "Guid" in node.attrib}
        upstream = {
            "0507D4B6-12FF-4882-84E1-59083E74A4EB",
            "5511327B-F305-41BE-92D0-EB84944E7C4D",
        }
        self.assertFalse(guids & upstream)


if __name__ == "__main__":
    unittest.main()
