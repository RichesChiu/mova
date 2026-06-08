#!/usr/bin/env python3
import json
import math
import statistics
import subprocess
import sys
from dataclasses import dataclass
from typing import List, Optional, Sequence, Tuple

SAMPLE_RATE = 8000
FRAME_SECONDS = 1
FRAME_SIZE = SAMPLE_RATE * FRAME_SECONDS
MIN_MATCH_SECONDS = 12
MAX_MATCH_SECONDS = 150
OFFSET_TOLERANCE_SECONDS = 18
FRAME_SIMILARITY_THRESHOLD = 0.93


@dataclass
class EpisodeInput:
    episode_number: int
    file_path: str


@dataclass
class PairCandidate:
    start_seconds: int
    end_seconds: int
    similarity: float
    episode_numbers: Tuple[int, int]


def emit(payload: dict) -> None:
    sys.stdout.write(json.dumps(payload))


def fail(reason: str) -> None:
    emit({"status": "no-match", "reason": reason})


def run_ffmpeg_extract(file_path: str, analysis_seconds: int) -> bytes:
    command = [
        "ffmpeg",
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        file_path,
        "-vn",
        "-ac",
        "1",
        "-ar",
        str(SAMPLE_RATE),
        "-t",
        str(analysis_seconds),
        "-f",
        "s16le",
        "-",
    ]
    result = subprocess.run(command, capture_output=True, check=False)
    if result.returncode != 0:
        stderr = result.stderr.decode("utf-8", errors="ignore").strip()
        raise RuntimeError(stderr or f"ffmpeg exited with status {result.returncode}")
    return result.stdout


def decode_pcm_mono_s16le(raw_bytes: bytes) -> List[int]:
    if len(raw_bytes) % 2 != 0:
        raw_bytes = raw_bytes[:-1]
    samples = []
    for index in range(0, len(raw_bytes), 2):
        value = int.from_bytes(raw_bytes[index : index + 2], "little", signed=True)
        samples.append(value)
    return samples


def window_samples(samples: Sequence[int]) -> List[Sequence[int]]:
    windows = []
    frame_count = len(samples) // FRAME_SIZE
    for index in range(frame_count):
        start = index * FRAME_SIZE
        end = start + FRAME_SIZE
        windows.append(samples[start:end])
    return windows


def goertzel_power(samples: Sequence[int], target_frequency: int) -> float:
    normalized_frequency = target_frequency / SAMPLE_RATE
    coefficient = 2.0 * math.cos(2.0 * math.pi * normalized_frequency)
    previous = 0.0
    previous2 = 0.0
    for sample in samples:
        current = sample + coefficient * previous - previous2
        previous2 = previous
        previous = current
    power = previous2 * previous2 + previous * previous - coefficient * previous * previous2
    return max(power, 0.0)


def build_frame_features(samples: Sequence[int]) -> List[float]:
    if not samples:
        return [0.0] * 8

    sample_count = len(samples)
    sum_squares = sum(sample * sample for sample in samples)
    rms = math.sqrt(sum_squares / sample_count)
    zero_crossings = 0
    previous_sign = 1 if samples[0] >= 0 else -1
    for sample in samples[1:]:
        sign = 1 if sample >= 0 else -1
        if sign != previous_sign:
            zero_crossings += 1
        previous_sign = sign
    mean_abs = sum(abs(sample) for sample in samples) / sample_count

    bands = [120, 240, 480, 960, 1920]
    band_powers = [goertzel_power(samples, frequency) for frequency in bands]
    total_band_power = sum(band_powers) or 1.0
    normalized_bands = [power / total_band_power for power in band_powers]

    return [
        math.log1p(rms),
        zero_crossings / sample_count,
        math.log1p(mean_abs),
        *normalized_bands,
    ]


def normalize_feature_vectors(vectors: List[List[float]]) -> List[List[float]]:
    if not vectors:
        return vectors

    dimensions = len(vectors[0])
    means = []
    deviations = []
    for dimension in range(dimensions):
        values = [vector[dimension] for vector in vectors]
        mean = statistics.fmean(values)
        deviation = statistics.pstdev(values)
        means.append(mean)
        deviations.append(deviation or 1.0)

    normalized = []
    for vector in vectors:
        normalized.append(
            [
                (value - means[index]) / deviations[index]
                for index, value in enumerate(vector)
            ]
        )
    return normalized


def cosine_similarity(left: Sequence[float], right: Sequence[float]) -> float:
    numerator = sum(a * b for a, b in zip(left, right))
    left_norm = math.sqrt(sum(a * a for a in left))
    right_norm = math.sqrt(sum(b * b for b in right))
    if left_norm == 0 or right_norm == 0:
        return 0.0
    return numerator / (left_norm * right_norm)


def load_episode_features(file_path: str, analysis_seconds: int) -> List[List[float]]:
    raw_audio = run_ffmpeg_extract(file_path, analysis_seconds)
    samples = decode_pcm_mono_s16le(raw_audio)
    windows = window_samples(samples)
    return normalize_feature_vectors([build_frame_features(window) for window in windows])


def detect_pair_candidate(
    left_episode_number: int,
    left_features: Sequence[Sequence[float]],
    right_episode_number: int,
    right_features: Sequence[Sequence[float]],
    max_start_offset_seconds: int,
    min_match_seconds: int,
) -> Optional[PairCandidate]:
    max_left_start = min(max_start_offset_seconds, len(left_features) - min_match_seconds)
    max_right_start = min(max_start_offset_seconds, len(right_features) - min_match_seconds)
    if max_left_start < 0 or max_right_start < 0:
        return None

    best_candidate: Optional[PairCandidate] = None
    for delta in range(-OFFSET_TOLERANCE_SECONDS, OFFSET_TOLERANCE_SECONDS + 1):
        left_start_min = max(0, -delta)
        left_start_max = min(max_left_start, max_right_start - delta)
        if left_start_max < left_start_min:
            continue

        for left_start in range(left_start_min, left_start_max + 1):
            right_start = left_start + delta
            run_length = 0
            similarities: List[float] = []
            max_length = min(
                len(left_features) - left_start,
                len(right_features) - right_start,
                MAX_MATCH_SECONDS,
            )

            for offset in range(max_length):
                similarity = cosine_similarity(
                    left_features[left_start + offset],
                    right_features[right_start + offset],
                )
                if similarity >= FRAME_SIMILARITY_THRESHOLD:
                    run_length += 1
                    similarities.append(similarity)
                    continue

                if run_length >= min_match_seconds:
                    candidate = PairCandidate(
                        start_seconds=round((left_start + right_start) / 2),
                        end_seconds=round((left_start + right_start) / 2) + run_length,
                        similarity=statistics.fmean(similarities),
                        episode_numbers=(left_episode_number, right_episode_number),
                    )
                    if best_candidate is None or (
                        (candidate.end_seconds - candidate.start_seconds)
                        > (best_candidate.end_seconds - best_candidate.start_seconds)
                    ) or (
                        (candidate.end_seconds - candidate.start_seconds)
                        == (best_candidate.end_seconds - best_candidate.start_seconds)
                        and candidate.similarity > best_candidate.similarity
                    ):
                        best_candidate = candidate

                run_length = 0
                similarities = []

            if run_length >= min_match_seconds:
                candidate = PairCandidate(
                    start_seconds=round((left_start + right_start) / 2),
                    end_seconds=round((left_start + right_start) / 2) + run_length,
                    similarity=statistics.fmean(similarities),
                    episode_numbers=(left_episode_number, right_episode_number),
                )
                if best_candidate is None or (
                    (candidate.end_seconds - candidate.start_seconds)
                    > (best_candidate.end_seconds - best_candidate.start_seconds)
                ) or (
                    (candidate.end_seconds - candidate.start_seconds)
                    == (best_candidate.end_seconds - best_candidate.start_seconds)
                    and candidate.similarity > best_candidate.similarity
                ):
                    best_candidate = candidate

    return best_candidate


def cluster_candidates(
    candidates: Sequence[PairCandidate], episode_count: int, min_match_seconds: int
) -> Optional[dict]:
    if not candidates:
        return None

    clusters = []
    for candidate in candidates:
        matched_cluster = None
        for cluster in clusters:
            if (
                abs(cluster["start_seconds"] - candidate.start_seconds) <= 6
                and abs(cluster["end_seconds"] - candidate.end_seconds) <= 6
            ):
                matched_cluster = cluster
                break

        if matched_cluster is None:
            matched_cluster = {
                "start_seconds": candidate.start_seconds,
                "end_seconds": candidate.end_seconds,
                "starts": [candidate.start_seconds],
                "ends": [candidate.end_seconds],
                "similarities": [candidate.similarity],
                "episodes": set(candidate.episode_numbers),
            }
            clusters.append(matched_cluster)
            continue

        matched_cluster["starts"].append(candidate.start_seconds)
        matched_cluster["ends"].append(candidate.end_seconds)
        matched_cluster["similarities"].append(candidate.similarity)
        matched_cluster["episodes"].update(candidate.episode_numbers)
        matched_cluster["start_seconds"] = round(statistics.median(matched_cluster["starts"]))
        matched_cluster["end_seconds"] = round(statistics.median(matched_cluster["ends"]))

    min_supported_episodes = max(3, math.ceil(episode_count * 0.6))
    ranked_clusters = []
    for cluster in clusters:
        supported_episodes = len(cluster["episodes"])
        if supported_episodes < min_supported_episodes:
            continue

        intro_duration = cluster["end_seconds"] - cluster["start_seconds"]
        if intro_duration < min_match_seconds:
            continue

        average_similarity = statistics.fmean(cluster["similarities"])
        support_ratio = supported_episodes / episode_count
        duration_ratio = min(intro_duration / 90.0, 1.0)
        confidence = max(0.0, min(1.0, average_similarity * 0.65 + support_ratio * 0.25 + duration_ratio * 0.10))
        ranked_clusters.append(
            (
                supported_episodes,
                intro_duration,
                average_similarity,
                confidence,
                cluster,
            )
        )

    if not ranked_clusters:
        return None

    ranked_clusters.sort(reverse=True)
    _, _, _, confidence, cluster = ranked_clusters[0]
    return {
        "intro_start_seconds": cluster["start_seconds"],
        "intro_end_seconds": cluster["end_seconds"],
        "confidence": round(confidence, 4),
    }


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError as error:
        emit({"status": "error", "reason": f"invalid request json: {error}"})
        return 1

    analysis_seconds = int(payload.get("analysis_seconds", 240))
    max_start_offset_seconds = int(payload.get("max_start_offset_seconds", 150))
    min_match_seconds = max(8, int(payload.get("min_intro_seconds", MIN_MATCH_SECONDS)))

    episodes = [
        EpisodeInput(
            episode_number=int(item["episode_number"]),
            file_path=str(item["file_path"]),
        )
        for item in payload.get("episodes", [])
    ]

    if len(episodes) < 3:
        fail("need at least three playable episodes")
        return 0

    episode_features = []
    for episode in episodes:
        try:
            features = load_episode_features(episode.file_path, analysis_seconds)
        except Exception as error:  # noqa: BLE001
            fail(f"failed to analyze {episode.file_path}: {error}")
            return 0

        if len(features) < min_match_seconds:
            fail(f"not enough audio frames for {episode.file_path}")
            return 0

        episode_features.append((episode, features))

    pair_candidates = []
    for left_index in range(len(episode_features)):
        left_episode, left_features = episode_features[left_index]
        for right_index in range(left_index + 1, len(episode_features)):
            right_episode, right_features = episode_features[right_index]
            candidate = detect_pair_candidate(
                left_episode.episode_number,
                left_features,
                right_episode.episode_number,
                right_features,
                max_start_offset_seconds,
                min_match_seconds,
            )
            if candidate is not None:
                pair_candidates.append(candidate)

    clustered = cluster_candidates(pair_candidates, len(episodes), min_match_seconds)
    if clustered is None:
        fail("no stable repeated intro segment detected")
        return 0

    emit(
        {
            "status": "ok",
            "intro_start_seconds": clustered["intro_start_seconds"],
            "intro_end_seconds": clustered["intro_end_seconds"],
            "confidence": clustered["confidence"],
        }
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
