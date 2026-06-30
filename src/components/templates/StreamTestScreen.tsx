import { audioUrl, docUrl, videoUrl } from "@/lib/stream";

export default function StreamTestScreen() {
  return (
    <>
      <video src={videoUrl(13947506944)} controls width="512" />
      <audio src={audioUrl(13947595380)} controls />
      <img src={docUrl(13916336906)} alt="Some doc" />
    </>
  );
}
