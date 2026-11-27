import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("verify", Path(__file__).with_name("verify-native-rtmps-decode.py"))
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)

class DecodeValidation(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.media = Path(self.temp.name)/"fixture.flv"
        self.media.write_bytes(b"FLV"+bytes(10))
        self.report = dict(orderlyClose=True,pongReceived=True,videoConfig=True,audioConfig=True,
                           mediaPackets=114,videoFrames=20,audioBlocks=92,
                           lastVideoMs=1900,lastAudioMs=round(91*1024/48))
        self.data = dict(streams=[
            dict(codec_type="video",codec_name="h264",width=640,height=360),
            dict(codec_type="audio",codec_name="aac",sample_rate="48000",channels=2)],
            frames=[dict(media_type="video",best_effort_timestamp_time=str(i/10)) for i in range(20)]+
            [dict(media_type="audio",best_effort_timestamp_time=str(round(i*1024/48)/1000),nb_samples=1024) for i in range(92)])

    def validate(self):
        with patch.object(verify,"run",side_effect=[json.dumps(self.data),""]) as run:
            result=verify.validate(self.media,self.report,"ffprobe","ffmpeg")
            self.assertIn("-xerror",run.call_args_list[1].args[0])
            return result

    def test_valid_summary(self):
        result=self.validate()
        self.assertEqual(result["decodedVideoFrames"],20)
        self.assertEqual(result["decodedAudioSamplesPerChannel"],92*1024)

    def test_reject_video_gap(self):
        self.data["frames"][10]["best_effort_timestamp_time"]="1.2"
        with self.assertRaisesRegex(RuntimeError,"Video timestamp gap"):
            self.validate()

    def test_reject_audio_gap(self):
        self.data["frames"][35]["best_effort_timestamp_time"]="0.5"
        with self.assertRaisesRegex(RuntimeError,"AAC timestamp gap"):
            self.validate()

    def test_reject_missing_frame(self):
        self.data["frames"].pop(10)
        with self.assertRaisesRegex(RuntimeError,"Decoded video frame count mismatch"):
            self.validate()

    def test_reject_missing_shutdown(self):
        self.report["orderlyClose"]=False
        with self.assertRaisesRegex(RuntimeError,"close cleanly"):
            self.validate()

    def test_reject_decoder_error(self):
        with patch.object(verify,"run",side_effect=[json.dumps(self.data),RuntimeError("decode failed")]):
            with self.assertRaisesRegex(RuntimeError,"decode failed"):
                verify.validate(self.media,self.report,"ffprobe","ffmpeg")

if __name__ == "__main__":
    unittest.main()
