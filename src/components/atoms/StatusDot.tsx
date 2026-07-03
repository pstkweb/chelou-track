import type { LessonStatus } from '@/lib/method-view';

type StatusDotProps = {
  status: LessonStatus;
};

export default function StatusDot({ status }: StatusDotProps) {
  return <span className={`sdot ${status}`} title={status} />;
}
