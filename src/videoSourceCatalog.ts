/** MyVideo lists only sources with verified search, playback, and offline download. */
export type VideoSourceCandidate = {
  id: string;
  name: string;
  homepage: string;
  exampleUrl: string;
  category: "anime" | "film-tv" | "documentary";
  integrationStatus: "verified" | "playback-beta";
};
export const videoSourceCandidates: readonly VideoSourceCandidate[] = [
  {
    id: "nnyy",
    name: "努努影院",
    homepage: "https://nnyy.in/",
    exampleUrl: "https://nnyy.in/dongman/20096378.html",
    category: "anime",
    integrationStatus: "verified",
  },
  {
    id: "kubb",
    name: "Kubb TV 酷播",
    homepage: "https://m.kubb.tv/",
    exampleUrl: "https://m.kubb.tv/play/qVyzCcnhWtG",
    category: "film-tv",
    integrationStatus: "verified",
  },
  {
    id: "ikanbot",
    name: "爱看机器人 · 播放测试",
    homepage: "https://www1.ikanbot.com/",
    exampleUrl: "https://www1.ikanbot.com/play/442095",
    category: "anime",
    integrationStatus: "playback-beta",
  },
];
