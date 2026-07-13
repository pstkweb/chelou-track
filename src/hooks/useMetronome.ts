import { useEffect, useRef } from 'react';

const metronome = new AudioContext();

function playWoodTick(ctx: AudioContext, when = ctx.currentTime, accent = false, volume = 1) {
  // volume=0 court-circuité avant de créer le moindre noeud : en plus d'éviter du travail
  // inutile, exponentialRampToValueAtTime refuse une cible exactement à 0 (RangeError).
  if (volume <= 0) {
    return;
  }

  // ---------- Oscillateur ----------
  const osc = ctx.createOscillator();
  osc.type = 'triangle';

  const oscGain = ctx.createGain();

  if (accent) {
    osc.frequency.setValueAtTime(2700, when);
    osc.frequency.exponentialRampToValueAtTime(850, when + 0.018);
    oscGain.gain.setValueAtTime(0.7 * volume, when);
  } else {
    osc.frequency.setValueAtTime(2000, when);
    osc.frequency.exponentialRampToValueAtTime(750, when + 0.018);
    oscGain.gain.setValueAtTime(0.45 * volume, when);
  }

  oscGain.gain.exponentialRampToValueAtTime(0.0001 * volume, when + 0.04);

  // ---------- Bruit ----------
  const buffer = ctx.createBuffer(1, ctx.sampleRate * 0.03, ctx.sampleRate);
  const data = buffer.getChannelData(0);

  for (let i = 0; i < data.length; i++) {
    data[i] = Math.random() * 2 - 1;
  }

  const noise = ctx.createBufferSource();
  noise.buffer = buffer;

  const noiseFilter = ctx.createBiquadFilter();
  noiseFilter.type = 'highpass';
  noiseFilter.frequency.value = 2500;

  const noiseGain = ctx.createGain();
  noiseGain.gain.setValueAtTime(0.18 * volume, when);
  noiseGain.gain.exponentialRampToValueAtTime(0.0001 * volume, when + 0.015);

  // ---------- Filtre global ----------
  const filter = ctx.createBiquadFilter();
  filter.type = 'bandpass';
  filter.frequency.value = accent ? 1600 : 1400;
  filter.Q.value = 5;

  // ---------- Connexions ----------
  osc.connect(oscGain);
  oscGain.connect(filter);

  noise.connect(noiseFilter);
  noiseFilter.connect(noiseGain);
  noiseGain.connect(filter);

  filter.connect(ctx.destination);

  osc.start(when);
  noise.start(when);

  osc.stop(when + 0.04);
  noise.stop(when + 0.03);
}

export default function useMetronome(bpm: number | undefined, enabled: boolean, volume = 1) {
  // Dernier temps joué (index absolu depuis le début du fichier) — évite de rejouer deux
  // fois le même temps sur des ticks rapprochés de onTimeUpdate (toutes les ~50ms).
  const lastBeatRef = useRef<number | null>(null);

  // bpm/enabled/volume tenus à jour via ref plutôt que lus directement dans tick() : tick()
  // est appelé depuis le setInterval de onTimeUpdate, capturé une seule fois dans onPlay —
  // sans ref, changer ces réglages en cours de lecture (bouton "Métronome", slider de mix)
  // resterait sans effet jusqu'à la prochaine pause/lecture, car cette closure figée ne
  // verrait jamais le nouvel état. Un ref est mis à jour à chaque render, quelle que soit
  // la closure qui le lit.
  const bpmRef = useRef(bpm);
  const enabledRef = useRef(enabled);
  const volumeRef = useRef(volume);
  useEffect(() => {
    bpmRef.current = bpm;
    enabledRef.current = enabled;
    volumeRef.current = volume;
  }, [bpm, enabled, volume]);

  // Appelé à chaque onTimeUpdate avec la position audio réelle (ms depuis le tout début du
  // fichier, pas depuis le lead-in) et l'ancrage du premier vrai temps du count-in
  // (countInStartMs, ou 0 tant qu'il n'est pas encore connu) — c'est ce qui garde le
  // métronome synchronisé avec play/pause/seek/changement de piste sans wiring séparé : pas
  // de tick tant que onTimeUpdate ne tourne pas (donc silence dès la pause), la position
  // réelle pilote directement quel temps doit sonner (resynchro immédiate après un seek), et
  // l'accent tombe sur le premier temps du count-in plutôt que sur un temps arbitraire décalé
  // par la durée (non alignée) du silence de tête.
  const tick = (elapsedMs: number, anchorMs: number, beatsPerBar = 4) => {
    if (!enabledRef.current || !bpmRef.current || elapsedMs < 0) {
      return;
    }

    const beatIndex = Math.floor((elapsedMs - anchorMs) / (60000 / bpmRef.current));
    if (beatIndex === lastBeatRef.current) {
      return;
    }
    lastBeatRef.current = beatIndex;

    const isDownbeat = beatsPerBar > 0 && beatIndex % beatsPerBar === 0;
    playWoodTick(metronome, metronome.currentTime, isDownbeat, volumeRef.current);
  };

  // À appeler quand la lecture redémarre depuis une nouvelle position (play, seek, reset) :
  // sans ça, si le nouveau beatIndex de départ coïncide par hasard avec le dernier joué
  // avant l'arrêt, le premier temps de la reprise serait silencieusement sauté.
  const reset = () => {
    lastBeatRef.current = null;
  };

  return { tick, reset };
}
