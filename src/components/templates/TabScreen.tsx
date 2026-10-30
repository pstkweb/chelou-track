import {
  ChevronLeft,
  LoaderCircle,
  Music,
  Pause,
  Play,
  RefreshCcw,
  SkipBack,
  SkipForward,
  Video,
} from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useNavigation } from '@/contexts/NavigationContext';
import useAlphaTabPlayer from '@/hooks/useAlphaTabPlayer';
import useBackingEngine from '@/hooks/useBackingEngine';
import useBackingTrackPlayback from '@/hooks/useBackingTrackPlayback';
import useLeadInDetection from '@/hooks/useLeadInDetection';
import cn from '@/lib/cn';
import { type Chapter, searchSiblings } from '@/lib/method-view';
import { metronomeParams } from '@/lib/metronome-grid';
import type { BackingGroup, Lesson, Method, TabSet } from '@/types/model';
import Button from '../atoms/Button';
import Chip from '../atoms/Chip';
import IconButton from '../atoms/IconButton';
import BackingTrackPicker from '../organisms/BackingTrackPicker';
import TabMixer from '../organisms/TabMixer';

type TabScreenProps = {
  chapter: Chapter;
  method: Method;
  lesson: Lesson;
  tab: TabSet;
};

function LoadError({ what, message }: { what: string; message: string | undefined }) {
  if (!message) {
    return null;
  }

  return (
    <div className="text-center text-fg2 text-sm">
      Impossible de charger {what} : {message}
    </div>
  );
}

// Le track est téléchargé et décodé avant de pouvoir jouer : sans état visible, un clic
// pendant le chargement était ignoré sans rien montrer.
function PlayButton({
  loading,
  ready,
  playing,
  onClick,
}: {
  loading: boolean;
  // Faux tant qu'aucun track n'est chargé — y compris après un échec, où le spinner n'a plus
  // lieu de tourner à côté du message d'erreur.
  ready: boolean;
  playing: boolean;
  onClick: () => void;
}) {
  let icon = <Play size={24} />;
  if (loading) {
    icon = <LoaderCircle size={24} className="animate-spin" />;
  } else if (playing) {
    icon = <Pause size={24} />;
  }

  return (
    <Button
      variant="primary"
      onClick={onClick}
      disabled={loading || !ready}
      title={loading ? 'Chargement de la backing track…' : undefined}
      className={cn(
        'size-13! rounded-full! border border-transparent p-0',
        (loading || !ready) && 'opacity-40',
      )}
    >
      {icon}
    </Button>
  );
}

export default function TabScreen({ lesson, chapter, method, tab }: TabScreenProps) {
  const [loop, setLoop] = useState(false);
  const [backingTrack, setBackingTrack] = useState<BackingGroup | undefined>(
    lesson.backingGroups[0] || undefined,
  );
  const [backingTrackSpeed, setBackingTrackSpeed] = useState(backingTrack?.tracks[0]);
  // Mix backing track / clic de métronome, 0-100 (échelle des sliders) — cf. wiring plus bas.

  const tabElmt = useRef<HTMLDivElement>(null);
  const engine = useBackingEngine(method.source.provider, backingTrackSpeed?.audio.fileId);

  const { openLesson } = useNavigation();
  const [previousLesson, nextLesson] = searchSiblings(chapter, lesson.order);
  const {
    alphaTabRef,
    beatsPerBarRef,
    beatsPerBar,
    notatedBpmRef,
    notatedBpm,
    scoreLoadedRef,
    loadError: tabLoadError,
    trackBpmRef,
    leadInMsRef,
  } = useAlphaTabPlayer(tabElmt, engine, tab.files[0]?.file.fileId, method.source.provider);
  // Certains backing tracks ont un BPM à 0 (tempo inconnu côté source) : on retombe alors
  // sur le tempo noté dans le fichier de tablature plutôt que de diviser par zéro partout.
  const effectiveBpm = backingTrackSpeed?.bpm || notatedBpm;
  const detectedLeadInMs = useLeadInDetection(
    method,
    lesson,
    backingTrackSpeed,
    engine.info,
    scoreLoadedRef,
    beatsPerBarRef,
    notatedBpmRef,
  );
  const { countIn, playing, resetPlayback, onTogglePlay } = useBackingTrackPlayback(
    alphaTabRef,
    engine,
    leadInMsRef,
    notatedBpmRef,
    beatsPerBarRef,
    method,
    backingTrackSpeed,
  );
  // Résultat de la détection pour la session en cours, tant que la persistance Rust
  // (fire-and-forget) n'a pas fait revivre backingTrackSpeed.leadInMsOverride via un
  // rechargement du Method — sinon un track jamais joué avant reste sans lead-in connu
  // jusqu'au prochain redémarrage de l'app, cf. bug rapporté (pas de count-in, tout désynchro).
  const effectiveLeadInMs = backingTrackSpeed?.leadInMsOverride ?? detectedLeadInMs;

  // Grille du métronome envoyée au moteur, qui joue le clic à l'échantillon près : renvoyée quand
  // le tempo, le lead-in ou la mesure changent, après chaque chargement de track (engine.info :
  // le moteur a pu changer de fréquence), et effacée en quittant la vue.
  // biome-ignore lint/correctness/useExhaustiveDependencies: setMetronome est un simple invoke ; engine.info signale un track (re)chargé
  useEffect(() => {
    engine.setMetronome(
      metronomeParams(effectiveBpm, effectiveLeadInMs, method.defaultCountInBars, beatsPerBar),
    );
  }, [effectiveBpm, effectiveLeadInMs, method.defaultCountInBars, beatsPerBar, engine.info]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: setMetronome est un simple invoke
  useEffect(
    () => () => {
      engine.setMetronome(null);
    },
    [],
  );

  // Tient à jour les refs que useAlphaTabPlayer.seekTo() lit pour convertir un seek (clic
  // sur la tablature) en position audio réelle — seekTo vit dans le hook et n'a pas accès
  // à cet état de composant autrement.
  useEffect(() => {
    trackBpmRef.current = effectiveBpm;
    leadInMsRef.current = effectiveLeadInMs;
  }, [effectiveBpm, effectiveLeadInMs, trackBpmRef, leadInMsRef]);

  return (
    <div className="grain relative flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-3.5 border-border border-b bg-bg2 px-[clamp(16px,3vw,32px)] py-3.5">
        <IconButton className="border border-border" onClick={() => openLesson(lesson, chapter)}>
          <ChevronLeft size={18} />
        </IconButton>
        <div className="type-ic tab size-9">
          <Music size={20} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="display text-xl">{tab.title}</div>
          <div className="mono text-fg3 text-xs">
            Chapitre {chapter.num} · {method.title}
          </div>
        </div>
        <div className="flex items-center gap-1.5">
          <Button
            className={cn('h-9 px-3 py-0 opacity-40', previousLesson && 'opacity-100')}
            disabled={!previousLesson}
            onClick={() => previousLesson && openLesson(previousLesson, chapter)}
            title={previousLesson ? previousLesson.title : 'Début du chapitre'}
          >
            <SkipBack size={16} /> Préc.
          </Button>
          <Button
            className={cn('h-9 px-3 py-0 opacity-40', nextLesson && 'opacity-100')}
            disabled={!nextLesson}
            onClick={() => nextLesson && openLesson(nextLesson, chapter)}
            title={nextLesson ? nextLesson.title : 'Fin du chapitre'}
          >
            Suiv. <SkipForward size={16} />
          </Button>
        </div>
        <Chip as="button" className="cursor-pointer" onClick={() => openLesson(lesson, chapter)}>
          <Video size={15} /> Revoir la vidéo
        </Chip>
      </div>

      <div className="scroll flex flex-1 justify-center p-[clamp(16px,3vw,40px)]">
        <div className="w-full">
          <div className="card bg-surface p-[clamp(16px,2.5vw,30px)]">
            <div className="mb-3.5 flex items-baseline justify-between">
              <div>
                <div className="display text-base">{tab.title}</div>
              </div>
            </div>
            <div className="flex flex-col gap-6">
              <LoadError what="la tablature" message={tabLoadError} />
              <LoadError what="la backing track" message={engine.error} />
              <div ref={tabElmt} className="scroll flex w-full flex-1 justify-center" />
            </div>
          </div>
        </div>
      </div>

      {countIn !== undefined && (
        <div className="absolute inset-0 z-20 flex items-center justify-center bg-mix-(--bg)/55 bg-mix-blur backdrop-blur-xs">
          <div className="display text-9xl text-accent">{countIn === 0 ? '--' : countIn}</div>
        </div>
      )}

      {lesson.backingGroups.length > 0 && (
        <div className="flex-initial border-border border-t bg-bg2 px-[clamp(16px,3vw,28px)] py-3">
          <div className="flex flex-wrap items-center gap-[clamp(12px,2vw,24px)]">
            <div className="flex items-center gap-1.5">
              <IconButton onClick={resetPlayback} title="Début">
                <SkipBack size={20} />
              </IconButton>
              <PlayButton
                loading={engine.loading}
                ready={engine.info !== undefined}
                playing={playing}
                onClick={onTogglePlay}
              />
            </div>

            <BackingTrackPicker
              backingGroups={lesson.backingGroups}
              selectedGroup={backingTrack}
              selectedTrack={backingTrackSpeed}
              notatedBpm={notatedBpm}
              onGroupSelect={setBackingTrack}
              onTrackSelect={(track) => {
                resetPlayback();
                setBackingTrackSpeed(track);
              }}
            />

            <div className="flex items-center gap-1">
              {false /* TODO */ && (
                <IconButton
                  onClick={() => setLoop((l) => !l)}
                  title="Répéter"
                  className={cn('bg-transparent text-fg3', loop && 'bg-chip! text-accent!')}
                >
                  <RefreshCcw size={19} />
                </IconButton>
              )}
            </div>

            <div className="ml-auto">
              <TabMixer />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
