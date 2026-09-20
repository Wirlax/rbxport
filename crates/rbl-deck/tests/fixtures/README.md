`seek-noise.mp3` is generated pink noise, not a music excerpt. It exercises
MP3 packet-timestamp alignment after forward, backward and zero seeks.
Regenerate with:

```sh
ffmpeg -v error -f lavfi \
  -i 'anoisesrc=color=pink:sample_rate=44100:duration=2:seed=42:amplitude=0.3' \
  -ac 2 -c:a libmp3lame -b:a 64k -y seek-noise.mp3
```

The test compares seeked PCM with a sequential decode of the same file;
checking the reported cursor alone misses the one-packet offset.
