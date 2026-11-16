/**
 * Candidate websites for the multi-source Android client.
 *
 * Registering a website here does NOT enable search, playback, or downloads.
 * Each website requires a separate adapter and a review of its allowed access
 * methods, media permissions, and any DRM restrictions.
 */
export type VideoSourceCandidate = {
  id: string;
  name: string;
  homepage: string;
  exampleUrl: string;
  category: "anime" | "film-tv" | "documentary";
  integrationStatus: "pending";
};

export const videoSourceCandidates: readonly VideoSourceCandidate[] = [
  {
    id: "nnyy",
    name: "nnyy.in",
    homepage: "https://nnyy.in/",
    exampleUrl: "https://nnyy.in/dongman/20096378.html",
    category: "anime",
    integrationStatus: "pending",
  },
  {
    id: "bandai-channel",
    name: "Bandai Channel",
    homepage: "https://www.b-ch.com/",
    exampleUrl: "https://www.b-ch.com/titles/2277/001",
    category: "anime",
    integrationStatus: "pending",
  },
  {
    id: "dm84",
    name: "dm84.tv",
    homepage: "https://dm84.tv/",
    exampleUrl: "https://dm84.tv/v/1773.html",
    category: "anime",
    integrationStatus: "pending",
  },
  {
    id: "iyf",
    name: "iyf.tv",
    homepage: "https://m.iyf.tv/",
    exampleUrl: "https://m.iyf.tv/play/qmM3z64VxiV?id=zXfHcY2NIAU",
    category: "film-tv",
    integrationStatus: "pending",
  },
  {
    id: "zhaojixie",
    name: "zhaojixie.com",
    homepage: "https://zhaojixie.com/",
    exampleUrl: "https://zhaojixie.com/category/jilupian.html",
    category: "documentary",
    integrationStatus: "pending",
  },
  {
    id: "ikanbot",
    name: "iKanBot (爱看机器人)",
    homepage: "https://www1.ikanbot.com/",
    exampleUrl: "https://www1.ikanbot.com/play/381898",
    category: "film-tv",
    integrationStatus: "pending",
  },
];
