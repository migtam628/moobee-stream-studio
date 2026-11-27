# Independent RTMPS receiver-file acceptance
Use this offline verifier for a locally saved, independently captured receiver file, preferably a remux of the received packets (not a transcode). Do not give the script a stream URL or a stream key. The application itself has no FFmpeg runtime dependency.
 
From the repository root, with Python 3, FFmpeg and FFprobe installed for QA:
 
```sh
python3 scripts/verify-rtmps-soak-capture.py received.flv --seconds 15 --platform macos-arm64 --profile 1080p30 --commit "$(git rev-parse HEAD)" > receiver-report.json
```
 
Use 300, 1800 or 7200 seconds for subsequent stages. Select the actual capture profile and the exact commit used by the sender, not the current checkout if different. FLV/MP4/MKV inputs must be local and no larger than 32 GiB. The verifier requires H.264 video, stereo 48 kHz AAC, nominal frame/packet cadence, monotonically increasing PTS, packet duration within two seconds of the requested test, a 200 ms maximum difference in encoded track endpoints, and independent full-file decoding with fail-on-error. This is a conservative diagnostic policy for the current no-B-frame encoder. Reordered/transcoded or container-incomplete files may fail and must be investigated rather than silently accepted. The threshold is not physical lip-sync measurement.
 
Collect the sender-side status, actual receiver observations, OS/hardware, exact commit, source selection, time/duration, authorization of the private destination, any recovery or disconnect events, and manual start/middle/end A/V synchronization checks. Never include credentials or a private destination URL. A successful JSON result is only offline file evidence; it never marks physical acceptance as verified or changes the production Go Live gate. A recovered or interrupted stream must not count as uninterrupted acceptance. Test reconnection separately. Review and link all evidence in issue #14/#18 before promotion.
 
CI tests packet-timing policy and a locally generated 15-second synthetic media fixture. That fixture is not a real MooBee broadcast or a physical capture.
