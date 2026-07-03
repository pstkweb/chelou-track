import { Maximize2, Minimize2, Pause, Play } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import useAutoHideControls from '@/hooks/useAutoHideControls';
import useFullscreen from '@/hooks/useFullscreen';
import useTimecodeResume from '@/hooks/useTimecodeResume';
import cn from '@/lib/cn';
import type { LessonStatus } from '@/lib/method-view';
import { videoUrl } from '@/lib/stream';
import IconButton from '../atoms/IconButton';

type VideoPlayerProps = {
  chapterNum: number;
  fileId: number;
  lessonId: string;
  lessonStatus: LessonStatus;
  methodId: string;
  methodTitle: string;
  resumeMs: number | undefined;
  onVideoEnd: () => void;
};

function fmtTime(seconds: number, usePlaceholder = false) {
  if (Number.isNaN(seconds) || seconds <= 0) {
    return usePlaceholder ? '-:--' : '0:00';
  }

  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);

  return `${m}:${s.toString().padStart(2, '0')}`;
}

export default function VideoPlayer({
  fileId,
  chapterNum,
  methodId,
  methodTitle,
  lessonId,
  lessonStatus,
  resumeMs,
  onVideoEnd,
}: VideoPlayerProps) {
  const { record: recordTimecode, cancel: cancelTimecode } = useTimecodeResume(methodId, lessonId);
  const [isPlaying, setIsPlaying] = useState(false);
  const [duration, setDuration] = useState(0);
  const [timecode, setTimecode] = useState(
    lessonStatus === 'current' && resumeMs ? Math.round(resumeMs / 1000) - 2 : 0,
  );
  const containerRef = useRef<HTMLDivElement | null>(null);
  const progressContainerRef = useRef<HTMLDivElement | null>(null);
  const videoRef = useRef<HTMLVideoElement | null>(null);

  const { isFullscreen, toggleFullscreen } = useFullscreen(containerRef);
  const { areControlsVisible, resetControlsTimer } = useAutoHideControls(isPlaying);

  const progressPct = (timecode / duration) * 100;
  const seekBy = (deltaInSeconds: number) => {
    if (videoRef.current) {
      resetControlsTimer();
      videoRef.current.currentTime = Math.max(0, videoRef.current.currentTime + deltaInSeconds);
    }
  };
  const handleScrub = (e: React.MouseEvent) => {
    if (progressContainerRef.current && videoRef.current) {
      const r = progressContainerRef.current.getBoundingClientRect();

      videoRef.current.currentTime =
        Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * duration;
    }
  };
  const handlePlay = () => {
    if (videoRef.current) {
      if (isPlaying) {
        videoRef.current.pause();
      } else {
        videoRef.current.play();
      }
      setIsPlaying(!isPlaying);
    }
  };
  const handleKeyboard = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const scrubModifier = e.shiftKey ? 10 : e.ctrlKey ? 1 : 5;

    switch (e.code) {
      case 'Space':
        e.preventDefault();
        handlePlay();
        break;
      case 'Escape':
        if (isFullscreen) {
          toggleFullscreen();
        }
        break;
      case 'KeyF':
        toggleFullscreen();
        break;
      case 'ArrowLeft':
        seekBy(-scrubModifier);
        break;
      case 'ArrowRight':
        seekBy(scrubModifier);
        break;
      default:
        break;
    }
  };
  const handleWatchProgress = (e: React.SyntheticEvent<HTMLVideoElement>) => {
    setTimecode(e.currentTarget.currentTime);

    recordTimecode(e.currentTarget.currentTime);
  };
  const handleMetadataLoaded = (e: React.SyntheticEvent<HTMLVideoElement>) => {
    setDuration(e.currentTarget.duration);

    if (lessonStatus === 'current' && resumeMs) {
      e.currentTarget.currentTime = Math.max(0, resumeMs / 1000 - 2);
    }
  };

  useEffect(() => {
    containerRef.current?.focus();
  }, []);

  return (
    <div
      className={cn(
        'video-container relative aspect-video outline-none',
        isFullscreen && 'fixed inset-0 z-9999 bg-black',
      )}
      onMouseMove={resetControlsTimer}
      onMouseEnter={resetControlsTimer}
      onKeyDown={handleKeyboard}
      ref={containerRef}
      // biome-ignore lint/a11y/noNoninteractiveTabindex: maybe temp
      tabIndex={0}
    >
      <video
        ref={videoRef}
        className={cn(
          'aspect-video overflow-hidden rounded-lg bg-(--media) shadow-soft',
          isFullscreen && 'aspect-auto h-full w-full rounded-none shadow-none',
        )}
        src={videoUrl(fileId)}
        controls={false}
        onLoadedMetadata={handleMetadataLoaded}
        onTimeUpdate={handleWatchProgress}
        onEnded={() => {
          cancelTimecode();
          onVideoEnd();
        }}
      />
      <div
        className={cn(
          'opacity-100 transition-opacity duration-200',
          !areControlsVisible && 'pointer-events-none opacity-0',
        )}
        onClick={handlePlay}
        onKeyUp={(e) => e.code === 'Enter' && handlePlay()}
      >
        <div className="absolute inset-0 bg-[radial-gradient(60%_60%_at_50%_45%,transparent,rgba(0,0,0,.4))]" />
        <div className="display absolute top-6 left-6 text-[rgba(255,255,255,.8)] text-sm tracking-widest">
          CHAPITRE {chapterNum} · {methodTitle.toUpperCase()}
        </div>
        <div className="absolute inset-0 flex items-center justify-center">
          <button
            type="button"
            className={cn(
              'flex size-20 items-center justify-center rounded-full border-2 border-[rgba(255,255,255,.4)] bg-[rgba(0,0,0,.4)] text-white backdrop-blur-sm transition-transform duration-200',
              isPlaying ? 'scale-75' : 'scale-90',
            )}
            onClick={handlePlay}
          >
            {isPlaying ? <Pause size={34} /> : <Play size={34} />}
          </button>
        </div>
        <div
          className="absolute right-0 bottom-0 left-0 bg-[linear-gradient(transparent,rgba(0,0,0,.55))] px-5 pt-0 pb-3.5"
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e: React.KeyboardEvent) => {
            if (e.key === 'Enter') e.stopPropagation();
          }}
        >
          <div
            ref={progressContainerRef}
            onClick={handleScrub}
            onKeyDown={() => {}}
            className="relative mb-0 h-1.5 cursor-pointer rounded-full bg-[rgba(255,255,255,.25)]"
          >
            <div className="h-full rounded-full bg-accent" style={{ width: `${progressPct}%` }} />
            <div
              className="-translate-1/2 absolute top-1/2 size-3.5 rounded-full bg-white shadow-[0_1px_4px_rgba(0,0,0,.4)]"
              style={{ left: `${progressPct}%` }}
            />
          </div>
          <div className="flex items-center gap-3.5 text-white">
            <IconButton className="text-white" onClick={handlePlay}>
              {isPlaying ? <Pause size={20} /> : <Play size={20} />}
            </IconButton>
            <span className="mono text-xs">
              {fmtTime(timecode, false)} / {fmtTime(duration)}
            </span>
            <div className="flex-1" />
            <IconButton className="text-white" onClick={toggleFullscreen}>
              {isFullscreen ? (
                <Minimize2 size={17} className="opacity-85" />
              ) : (
                <Maximize2 size={17} className="opacity-85" />
              )}
            </IconButton>
          </div>
        </div>
      </div>
    </div>
  );
}
