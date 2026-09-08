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
  /**
   * Seconds elapsed, as React state — updated ten times a second, which is as
   * fine as the readouts get and as often as the waveform is worth redrawing.
   *
   * The playhead does not use this: see `subscribe`.
   */
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
  /** Where playback is right now, without waiting for a render. */
  positionRef: React.RefObject<number>;
  /**
   * Every frame while playing, and once on each seek.
   *
   * The playhead is driven from here rather than from `position`: state feeds
   * the whole player subtree, so a faster tick only re-renders it faster. A
   * listener writes a transform straight to its own element instead.
   *
   * Returns its own unsubscribe.
   */
  subscribe: (listener: (seconds: number) => void) => () => void;
}

/**
 * How often the React readouts are updated, in seconds.
 *
 * The time is printed to a tenth, so ten a second is every digit it can show.
 * The playhead moves on every frame regardless — it goes through `subscribe`.
 */
const STATE_TICK = 0.1;

export function usePlayback(trackId: string | null): Playback {
  const element = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);

  // The live position, and who wants it every frame.
  const positionRef = useRef(0);
  const listeners = useRef(new Set<(seconds: number) => void>());

  const emit = useCallback((seconds: number) => {
    positionRef.current = seconds;
    for (const listener of listeners.current) listener(seconds);
  }, []);

  const subscribe = useCallback((listener: (seconds: number) => void) => {
    listeners.current.add(listener);
    return () => {
      listeners.current.delete(listener);
    };
  }, []);

  // One element for the life of the component, re-pointed as the track
  // changes: creating one per track leaks decoders and restarts buffering.
  useEffect(() => {
    const audio = new Audio();
    audio.preload = "metadata";
    element.current = audio;

    // `timeupdate` fires about four times a second, which is what made the
    // playhead step; while playing, the frame loop below is what moves it, and
    // this stays as the backstop for a position that changed some other way.
    const onTime = () => {
      setPosition(audio.currentTime);
      emit(audio.currentTime);
    };
    const onMeta = () => setDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onEnded = () => {
      setPlaying(false);
      setPosition(0);
      emit(0);
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
  }, [emit]);

  // One frame loop for the whole player, running only while audio is, so an
  // idle window schedules nothing.
  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    let lastState = positionRef.current;
    const tick = () => {
      frame = requestAnimationFrame(tick);
      const audio = element.current;
      if (!audio) return;
      const now = audio.currentTime;
      emit(now);
      if (Math.abs(now - lastState) >= STATE_TICK) {
        lastState = now;
        setPosition(now);
      }
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing, emit]);

  // Point it at the selected track. Loading does not start playback: choosing
  // a track in the browser should not make noise.
  useEffect(() => {
    const audio = element.current;
    if (!audio) return;
    const url = trackId === null ? undefined : audioUrl(trackId);
    setPosition(0);
    emit(0);
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
  }, [trackId, emit]);

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
    // Straight away, rather than on the next frame: a seek while paused
    // schedules no frame at all, and the head would sit where it was.
    emit(audio.currentTime);
  }, [emit]);

  const seekFraction = useCallback(
    (fraction: number) => {
      if (duration <= 0) return;
      seek(Math.min(Math.max(fraction, 0), 1) * duration);
    },
    [duration, seek],
  );

  return { playing, position, duration, idle, error, toggle, seek, seekFraction, positionRef, subscribe };
}
