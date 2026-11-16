# Multi-site video search — product requirement

## User goal
This Android app must be a general-purpose, **multi-site video search, playback and permitted download** client. The example pages in `src/videoSourceCatalog.ts` (e.g. an Inuyasha page) are only seed URLs used to recognize/test a website. **They are not the entire catalogue, a search index, or a fixed whitelist of titles.** Do not build a title-specific app.

## Six candidate sources
- `nnyy.in`
- `b-ch.com`
- `dm84.tv`
- `m.iyf.tv`
- `zhaojixie.com`
- `www1.ikanbot.com`

All integrations require separate verification of the source's available search endpoints, authorization and terms. Initial registration does not imply a working connector.

## Required search behavior
1. User enters any video title or keyword (Chinese, Japanese, English where supported).
2. Offer **All sources** (default) or choose one source.
3. Query each enabled, supported site adapter dynamically, NOT static example URLs.
4. Merge responses into one results view. Display poster, title, source, type (if available), and detail URL; distinguish identical titles from different sites rather than silently dropping results.
5. Allow users to open details and browse episodes or available playback lines.
6. Failure of one source must not block others. Show per-site statuses: searching / results / no matches / unsupported / error.
7. Support pagination / more results as available for each adapter.
8. Selecting a playable episode opens in-app playback where its media access is authorized and technically supported.
9. Enable downloading only for videos that may be downloaded under the source's permissions, using suitable accessible MP4 or HLS streams. Do not bypass DRM, paywalls, authentication requirements, or technical protections. Show why download is unavailable when necessary.
10. Local downloads should support progress, cancel, persistent file location, and offline playback, once implemented.

## Proposed adapter interface
```ts
type SourceSearchResult = {
  sourceId: string;
  title: string;
  detailUrl: string;
  posterUrl?: string;
  description?: string;
};

type SourceAdapter = {
  sourceId: string;
  search(query: string, page: number): Promise<{
    items: SourceSearchResult[];
    hasMore: boolean;
  }>;
  // Later: detail, playback, permitted download
};
```

## Implementation status
- [x] Six website candidates and example URLs recorded in `src/videoSourceCatalog.ts`
- [x] Explicit requirement: dynamic search across all relevant titles, not only linked examples
- [ ] Site-specific search APIs and response parsers
- [ ] Federated search UI and results merging
- [ ] Site-specific details and playback
- [ ] Permitted downloads and offline library
- [ ] Real-device tests of Android APK

This document is a requirement, not a claim that the listed capabilities already work.
