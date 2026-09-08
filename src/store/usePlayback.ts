/**
 * Playback for the preview player.
 *
 * An `<audio>` element rather than an audio stack in Rust: the webview already
 * decodes every format the library holds, buffers off the UI thread, and gives
 * seeking for free over the range requests the backend answers. A Rust output
 * path would buy lower latency, which a preview player does not need.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { audioUrl, canPlay } from "@/ipc/audio";

export interface Playback {
  /** True while audio is actually running. */
  playing: boolean;
  /** Seconds elapsed. */
  position: number;
  /** Seconds total, or 0 before metadata arrives. */
  duration: number;
  /** Nothing to play: no track, or a build with no backend. */
  idle: boolean;
  error: string | null;
  toggle: () => void;
  seek: (seconds: number) => void;
  /** Seek by fraction, for clicking the waveform. */
  seekFraction: (fraction: number) => void;
}

export function usePlayback(trackId: string | null): Playback {
  const element = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);

  // One element for the life of the component, re-pointed as the track
  // changes: creating one per track leaks decoders and restarts buffering.
  useEffect(() => {
    const audio = new Audio();
    audio.preload = "metadata";
    element.current = audio;

    const onTime = () => setPosition(audio.currentTime);
    const onMeta = () => setDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onEnded = () => {
      setPlaying(false);
      setPosition(0);
    };
    const onError = () => {
      setPlaying(false);
      setError("This track could not be played.");
    };

    audio.addEventListener("timeupdate", onTime);
    audio.addEventListener("loadedmetadata", onMeta);
    audio.addEventListener("play", onPlay);
    audio.addEventListener("pause", onPause);
    audio.addEventListener("ended", onEnded);
    audio.addEventListener("error", onError);
    return () => {
      audio.pause();
      // Dropping the source lets the decoder go rather than keeping the file
      // open for the life of the window.
      audio.removeAttribute("src");
      audio.load();
      audio.removeEventListener("timeupdate", onTime);
      audio.removeEventListener("loadedmetadata", onMeta);
      audio.removeEventListener("play", onPlay);
      audio.removeEventListener("pause", onPause);
      audio.removeEventListener("ended", onEnded);
      audio.removeEventListener("error", onError);
      element.current = null;
    };
  }, []);

  // Point it at the selected track. Loading does not start playback: choosing
  // a track in the browser should not make noise.
  useEffect(() => {
    const audio = element.current;
    if (!audio) return;
    const url = trackId === null ? undefined : audioUrl(trackId);
    setPosition(0);
    setDuration(0);
    setError(null);
    if (url === undefined) {
      audio.pause();
      audio.removeAttribute("src");
      audio.load();
      return;
    }
    audio.src = url;
    audio.load();
  }, [trackId]);

  const idle = !canPlay || trackId === null;

  const toggle = useCallback(() => {
    const audio = element.current;
    if (!audio || idle) return;
    if (audio.paused) {
      // A rejected play() is normal — an unreadable file, or a policy block —
      // and must surface rather than leave the button looking stuck.
      void audio.play().catch(() => setError("This track could not be played."));
    } else {
      audio.pause();
    }
  }, [idle]);

  const seek = useCallback((seconds: number) => {
    const audio = element.current;
    if (!audio || !Number.isFinite(seconds)) return;
    audio.currentTime = Math.max(0, seconds);
    setPosition(audio.currentTime);
  }, []);

  const seekFraction = useCallback(
    (fraction: number) => {
      if (duration <= 0) return;
      seek(Math.min(Math.max(fraction, 0), 1) * duration);
    },
    [duration, seek],
  );

  return { playing, position, duration, idle, error, toggle, seek, seekFraction };
}
