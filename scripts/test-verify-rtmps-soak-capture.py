import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("soak", Path(__file__).with_name("verify-rtmps-soak-capture.py"))
soak = importlib.util.module_from_spec(spec)
spec.loader.exec_module(soak)

def fixture(seconds=15, offset=0, drop=None, fps=30):
    lines=[]
    for index, cadence in [(0, 1/fps), (1, 1024/48000)]:
        for i in range(round(seconds/cadence)):
            if drop and index==0 and i in drop:
                continue
            t=i*cadence+(offset if index==1 else 0)
            lines.append(f"stream_index={index}|pts_time={t:.6f}|duration_time={cadence:.6f}")
    return lines

class SoakTests(unittest.TestCase):
    def test_nominal_timing(self):
        result=soak.evaluate(*soak.scan_packets(fixture(),0,1),15,30)
        self.assertTrue(result["packetTimingPass"])
    def test_wrong_profile_cadence(self):
        for fps in (24, 60):
            result=soak.evaluate(*soak.scan_packets(fixture(fps=fps),0,1),15,30)
            self.assertFalse(result["packetTimingPass"])
            self.assertTrue(any("observed packet rate" in x for x in result["issues"]))
        self.assertTrue(soak.evaluate(*soak.scan_packets(fixture(fps=30),0,1),15,30)["packetTimingPass"])
    def test_long_gap(self):
        result=soak.evaluate(*soak.scan_packets(fixture(drop=range(20,30)),0,1),15,30)
        self.assertFalse(result["packetTimingPass"])
        self.assertTrue(any("packet gap" in x for x in result["issues"]))
    def test_offset_and_short_file(self):
        self.assertFalse(soak.evaluate(*soak.scan_packets(fixture(offset=.5),0,1),15,30)["packetTimingPass"])
        self.assertFalse(soak.evaluate(*soak.scan_packets(fixture(seconds=10),0,1),15,30)["packetTimingPass"])
    def test_missing_and_duplicate_timestamps(self):
        data=fixture()+["stream_index=0|pts_time=N/A|duration_time=N/A",
                        "stream_index=1|pts_time=0.0|duration_time=.021333"]
        self.assertFalse(soak.evaluate(*soak.scan_packets(data,0,1),15,30)["packetTimingPass"])

if __name__=="__main__":
    unittest.main()
