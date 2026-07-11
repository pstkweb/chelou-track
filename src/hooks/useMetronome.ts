import { useRef } from 'react';

const metronome = new AudioContext();

export default function useMetronome(bpm: number | undefined, enabled: boolean) {
  const metronomeInterval = useRef<number>(null);
  const makeTickSound = () => {
    const sound = metronome.createOscillator();

    sound.frequency.value = 440;
    sound.connect(metronome.destination);
    sound.start(metronome.currentTime);
    sound.stop(metronome.currentTime + 0.1);
  };

  const start = () => {
    if (enabled && bpm) {
      makeTickSound();

      metronomeInterval.current = window.setInterval(() => {
        makeTickSound();
      }, 60000 / bpm);
    }
  };

  const stop = () => {
    if (metronomeInterval.current) {
      clearInterval(metronomeInterval.current);
      metronomeInterval.current = null;
    }
  };

  return { start, stop };
}
