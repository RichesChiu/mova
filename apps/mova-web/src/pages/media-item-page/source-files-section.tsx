import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { listMediaFileAudioTracks, listMediaFileSubtitles } from '../../api/client'
import type { MediaFile } from '../../api/types'
import { GlassSelect } from '../../components/glass-select'
import { SectionHelp } from '../../components/section-help'
import { useI18n } from '../../i18n'
import {
  buildAudioTrackFacts,
  buildAudioTrackOptions,
  buildSubtitleTrackFacts,
  buildSubtitleTrackOptions,
  buildVideoCardFacts,
  getMediaFileDisplayName,
} from '../../lib/media-file-details'
import { MEDIA_DETAIL_QUERY_STALE_TIME_MS, MEDIA_QUERY_GC_TIME_MS } from '../../lib/query-options'

interface MediaItemSourceFilesSectionProps {
  error: unknown
  isLoading: boolean
  isSeriesView: boolean
  mediaFiles: MediaFile[]
  originalTitle: string | null
  selectedMediaFile: MediaFile | null
  sourceContextDescription: string | null
}

export const MediaItemSourceFilesSection = ({
  error,
  isLoading,
  isSeriesView,
  mediaFiles,
  originalTitle,
  selectedMediaFile,
  sourceContextDescription,
}: MediaItemSourceFilesSectionProps) => {
  const { l } = useI18n()
  const [selectedAudioTrackId, setSelectedAudioTrackId] = useState('')
  const [selectedSubtitleTrackId, setSelectedSubtitleTrackId] = useState('')
  const audioTracksQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: selectedMediaFile !== null,
    queryKey: ['media-file-audio-tracks', selectedMediaFile?.id],
    queryFn: () => listMediaFileAudioTracks(selectedMediaFile?.id ?? 0),
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })
  const subtitleTracksQuery = useQuery({
    gcTime: MEDIA_QUERY_GC_TIME_MS,
    enabled: selectedMediaFile !== null,
    queryKey: ['media-file-subtitles', selectedMediaFile?.id],
    queryFn: () => listMediaFileSubtitles(selectedMediaFile?.id ?? 0),
    staleTime: MEDIA_DETAIL_QUERY_STALE_TIME_MS,
  })

  const audioTracks = audioTracksQuery.data ?? []
  const subtitleTracks = subtitleTracksQuery.data ?? []
  const audioTrackOptions = buildAudioTrackOptions(audioTracks)
  const selectedAudioTrackValue = selectedAudioTrackId || audioTrackOptions[0]?.value || ''
  const selectedAudioTrack =
    audioTracks.find((track) => String(track.id) === selectedAudioTrackValue) ??
    audioTracks[0] ??
    null
  const subtitleTrackOptions = buildSubtitleTrackOptions(subtitleTracks)
  const selectedSubtitleTrackValue = selectedSubtitleTrackId || subtitleTrackOptions[0]?.value || ''
  const selectedSubtitleTrack =
    subtitleTracks.find((track) => String(track.id) === selectedSubtitleTrackValue) ??
    subtitleTracks[0] ??
    null
  const selectedSubtitleTrackIndex = selectedSubtitleTrack
    ? subtitleTracks.findIndex((track) => track.id === selectedSubtitleTrack.id)
    : -1

  return (
    <section className="page-stack">
      <div className="section-heading">
        <div className="media-item-section-title-row">
          <h3>{l('Source Files')}</h3>
          {isSeriesView && sourceContextDescription ? (
            <SectionHelp
              detail={sourceContextDescription}
              placement="bottom"
              title={l('Source file selection help')}
              variant="notice"
            />
          ) : null}
        </div>
        {!isLoading && !error ? <span className="counter-badge">{mediaFiles.length}</span> : null}
      </div>

      <div className="season-card media-file-panel">
        {isLoading ? <p className="muted">{l('Loading source details…')}</p> : null}
        {error ? (
          <p className="callout callout--danger">
            {error instanceof Error ? error.message : l('Failed to load source details')}
          </p>
        ) : null}

        {!isLoading && !error && mediaFiles.length > 0 && selectedMediaFile ? (
          <div className="media-file-panel__list">
            <div className="media-file-detail">
              {originalTitle ? (
                <div className="media-file-detail__meta-block">
                  <p className="media-file-detail__label">{l('Original title')}</p>
                  <p className="media-file-detail__meta-value">{originalTitle}</p>
                </div>
              ) : null}
              <div className="media-file-detail__path-block">
                <p className="media-file-detail__label">{l('Path')}</p>
                <p className="media-file-detail__path">{selectedMediaFile.file_path}</p>
              </div>
            </div>

            <div className="media-tech-stack">
              <article className="media-tech-card media-tech-card--video">
                <div className="media-tech-card__header">
                  <div className="media-tech-card__title-block">
                    <p className="media-tech-card__eyebrow">{l('Video')}</p>
                    <h5>{l('Video Details')}</h5>
                  </div>
                </div>
                <dl className="media-tech-card__facts">
                  {buildVideoCardFacts(selectedMediaFile).map((fact) => (
                    <div
                      className="media-tech-card__fact"
                      key={`${selectedMediaFile.id}-video-${fact.label}`}
                    >
                      <dt>{fact.label}</dt>
                      <dd>{fact.value}</dd>
                    </div>
                  ))}
                </dl>
              </article>

              <article className="media-tech-card">
                <div className="media-tech-card__header media-tech-card__header--with-select">
                  <div className="media-tech-card__title-block">
                    <p className="media-tech-card__eyebrow">{l('Audio')}</p>
                    <h5>{l('Audio Details')}</h5>
                  </div>
                  <div className="media-tech-card__selector">
                    <GlassSelect
                      ariaLabel={l('Select audio track for {{name}}', {
                        name: getMediaFileDisplayName(selectedMediaFile.file_path),
                      })}
                      compact
                      disabled={audioTrackOptions.length === 0}
                      onChange={setSelectedAudioTrackId}
                      options={
                        audioTrackOptions.length > 0
                          ? audioTrackOptions
                          : [
                              {
                                label: l('No audio tracks detected'),
                                value: `empty-audio-${selectedMediaFile.id}`,
                              },
                            ]
                      }
                      value={
                        audioTrackOptions.length > 0
                          ? selectedAudioTrackValue
                          : `empty-audio-${selectedMediaFile.id}`
                      }
                    />
                  </div>
                </div>
                {audioTracksQuery.isLoading ? (
                  <p className="muted">{l('Loading audio tracks…')}</p>
                ) : null}
                {audioTracksQuery.isError ? (
                  <p className="callout callout--danger">
                    {audioTracksQuery.error instanceof Error
                      ? audioTracksQuery.error.message
                      : l('Failed to load audio tracks')}
                  </p>
                ) : null}
                {!audioTracksQuery.isLoading && !audioTracksQuery.isError && selectedAudioTrack ? (
                  <dl className="media-tech-card__facts">
                    {buildAudioTrackFacts(selectedAudioTrack).map((fact) => (
                      <div
                        className="media-tech-card__fact"
                        key={`${selectedAudioTrack.id}-${fact.label}`}
                      >
                        <dt>{fact.label}</dt>
                        <dd>{fact.value}</dd>
                      </div>
                    ))}
                  </dl>
                ) : null}
                {!audioTracksQuery.isLoading && !audioTracksQuery.isError && !selectedAudioTrack ? (
                  <p className="muted">{l('No embedded audio tracks were detected.')}</p>
                ) : null}
              </article>

              <article className="media-tech-card">
                <div className="media-tech-card__header media-tech-card__header--with-select">
                  <div className="media-tech-card__title-block">
                    <p className="media-tech-card__eyebrow">{l('Subtitle')}</p>
                    <h5>{l('Subtitle Details')}</h5>
                  </div>
                  <div className="media-tech-card__selector">
                    <GlassSelect
                      ariaLabel={l('Select subtitle track for {{name}}', {
                        name: getMediaFileDisplayName(selectedMediaFile.file_path),
                      })}
                      compact
                      disabled={subtitleTrackOptions.length === 0}
                      onChange={setSelectedSubtitleTrackId}
                      options={
                        subtitleTrackOptions.length > 0
                          ? subtitleTrackOptions
                          : [
                              {
                                label: l('No subtitles detected'),
                                value: `empty-subtitle-${selectedMediaFile.id}`,
                              },
                            ]
                      }
                      value={
                        subtitleTrackOptions.length > 0
                          ? selectedSubtitleTrackValue
                          : `empty-subtitle-${selectedMediaFile.id}`
                      }
                    />
                  </div>
                </div>
                {subtitleTracksQuery.isLoading ? (
                  <p className="muted">{l('Loading subtitles…')}</p>
                ) : null}
                {subtitleTracksQuery.isError ? (
                  <p className="callout callout--danger">
                    {subtitleTracksQuery.error instanceof Error
                      ? subtitleTracksQuery.error.message
                      : l('Failed to load subtitles')}
                  </p>
                ) : null}
                {!subtitleTracksQuery.isLoading &&
                !subtitleTracksQuery.isError &&
                selectedSubtitleTrack ? (
                  <dl className="media-tech-card__facts">
                    {buildSubtitleTrackFacts(
                      selectedSubtitleTrack,
                      selectedSubtitleTrackIndex >= 0 ? selectedSubtitleTrackIndex : 0,
                    ).map((fact) => (
                      <div
                        className="media-tech-card__fact"
                        key={`${selectedSubtitleTrack.id}-${fact.label}`}
                      >
                        <dt>{fact.label}</dt>
                        <dd>{fact.value}</dd>
                      </div>
                    ))}
                  </dl>
                ) : null}
                {!subtitleTracksQuery.isLoading &&
                !subtitleTracksQuery.isError &&
                !selectedSubtitleTrack ? (
                  <p className="muted">{l('No subtitle tracks were detected.')}</p>
                ) : null}
              </article>
            </div>
          </div>
        ) : null}

        {!isLoading && !error && mediaFiles.length === 0 ? (
          <p className="muted">
            {isSeriesView
              ? l('No source files are linked to the selected season episode yet.')
              : l('No source files are linked to this title yet.')}
          </p>
        ) : null}
      </div>
    </section>
  )
}
