import type { synth } from '@coderline/alphatab';
import {
  ChevronLeft,
  Metronome,
  Music,
  Pause,
  Play,
  RefreshCcw,
  SkipBack,
  SkipForward,
  Video,
} from 'lucide-react';
import { useRef, useState } from 'react';
import { useNavigation } from '@/contexts/NavigationContext';
import useAlphaTabPlayer from '@/hooks/useAlphaTabPlayer';
import useMetronome from '@/hooks/useMetronome';
import cn from '@/lib/cn';
import { type Chapter, searchSiblings } from '@/lib/method-view';
import { audioUrl } from '@/lib/stream';
import type { BackingGroup, Lesson, Method, TabSet } from '@/types/model';
import Button from '../atoms/Button';
import Chip from '../atoms/Chip';
import IconButton from '../atoms/IconButton';
import BackingTrackPicker from '../organisms/BackingTrackPicker';

type TabScreenProps = {
  chapter: Chapter;
  method: Method;
  lesson: Lesson;
  tab: TabSet;
};

export default function TabScreen({ lesson, chapter, method, tab }: TabScreenProps) {
  const [playing, setPlaying] = useState(false);
  const [loop, setLoop] = useState(false);
  const [metro, setMetro] = useState(true);
  const [backingTrack, setBackingTrack] = useState<BackingGroup | undefined>(
    lesson.backingGroups[0] || undefined,
  );
  const [backingTrackSpeed, setBackingTrackSpeed] = useState(backingTrack?.tracks[0]);

  const tabElmt = useRef<HTMLDivElement>(null);
  const audioElmt = useRef<HTMLAudioElement>(null);
  const updateTimer = useRef<number>(null);

  const { openLesson } = useNavigation();
  const [previousLesson, nextLesson] = searchSiblings(chapter, lesson.order);
  const alphaTabRef = useAlphaTabPlayer(tabElmt, audioElmt, tab.files[0]?.file.fileId);
  const { start: startMetronome, stop: stopMetronome } = useMetronome(
    backingTrackSpeed?.bpm,
    metro,
  );

  const onTimeUpdate = () => {
    const audio = audioElmt.current;
    const api = alphaTabRef.current;

    const player = api?.player;

    if (!audio || !player) {
      return;
    }

    // updatePosition (pas tickPosition : son setter déclenche un vrai seek à chaque appel,
    // ce qui interrompt la lecture au lieu de se contenter de déplacer le curseur)
    (player.output as unknown as synth.IExternalMediaSynthOutput).updatePosition(
      audio.currentTime * 1000,
    );
  };

  const onPlay = () => {
    const api = alphaTabRef.current;

    if (!api) {
      return;
    }

    api.play();
    updateTimer.current = window.setInterval(onTimeUpdate, 50);

    startMetronome();
  };

  const onPause = () => {
    const api = alphaTabRef.current;

    if (api) {
      api.pause();
    }

    if (updateTimer.current) {
      clearInterval(updateTimer.current);
      updateTimer.current = null;
    }

    stopMetronome();
  };

  const onVolumeChange = () => {
    const audio = audioElmt.current;
    const api = alphaTabRef.current;

    if (!audio || !api) {
      return;
    }

    api.masterVolume = audio.volume;
  };

  const onRateChange = () => {
    const audio = audioElmt.current;
    const api = alphaTabRef.current;

    if (!audio || !api) {
      return;
    }

    api.playbackSpeed = audio.playbackRate;
  };

  const stopPlayback = () => {
    setPlaying(false);
    audioElmt.current?.pause();
  };

  const onTogglePlay = () => {
    if (playing) {
      stopPlayback();
    } else {
      setPlaying(true);
      audioElmt.current?.play();
    }
  };

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
              <div ref={tabElmt} className="scroll flex w-full flex-1 justify-center" />
            </div>
          </div>
          {backingTrackSpeed && (
            <audio
              className="hidden"
              src={audioUrl(backingTrackSpeed.audio.fileId)}
              ref={audioElmt}
              onTimeUpdate={onTimeUpdate}
              onSeeked={onTimeUpdate}
              onPlay={onPlay}
              onPause={onPause}
              onEnded={onPause}
              onVolumeChange={onVolumeChange}
              onRateChange={onRateChange}
            />
          )}
        </div>
      </div>

      {lesson.backingGroups.length > 0 && (
        <div className="flex-initial border-border border-t bg-bg2 px-[clamp(16px,3vw,28px)] py-3">
          <div className="flex flex-wrap items-center gap-[clamp(12px,2vw,24px)]">
            <div className="flex items-center gap-1.5">
              <Button
                variant="primary"
                onClick={onTogglePlay}
                className="size-13! rounded-full! border border-transparent p-0"
              >
                {playing ? <Pause size={24} /> : <Play size={24} />}
              </Button>
            </div>

            <BackingTrackPicker
              backingGroups={lesson.backingGroups}
              selectedGroup={backingTrack}
              selectedTrack={backingTrackSpeed}
              onGroupSelect={setBackingTrack}
              onTrackSelect={(track) => {
                stopPlayback();
                setBackingTrackSpeed(track);
              }}
            />

            <div className="flex items-center gap-1">
              <IconButton
                onClick={() => setLoop((l) => !l)}
                title="Répéter"
                className={cn('bg-transparent text-fg3', loop && 'bg-chip! text-accent!')}
              >
                <RefreshCcw size={19} />
              </IconButton>
              <IconButton
                onClick={() => setMetro((m) => !m)}
                title="Métronome"
                className={cn('bg-transparent text-fg3', metro && 'bg-chip! text-accent!')}
              >
                <Metronome size={19} />
              </IconButton>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
