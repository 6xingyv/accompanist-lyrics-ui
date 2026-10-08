SELECT 'clock counters' AS section;
SELECT t.name, COUNT(*) AS samples, MIN(c.value) AS min_ms,
  ROUND(AVG(c.value), 3) AS mean_ms, PERCENTILE(c.value, 50) AS p50_ms,
  PERCENTILE(c.value, 95) AS p95_ms, PERCENTILE(c.value, 99) AS p99_ms,
  MAX(c.value) AS max_ms
FROM counter c JOIN counter_track t ON c.track_id = t.id
WHERE t.name GLOB 'Playback.*Ms' OR t.name = 'Lyrics.timelineLagMs'
GROUP BY t.name ORDER BY t.name;

SELECT 'lyrics CPU slices' AS section;
SELECT name, COUNT(*) AS samples, ROUND(AVG(dur)/1e6, 4) AS mean_ms,
  ROUND(PERCENTILE(dur, 50)/1e6, 4) AS p50_ms,
  ROUND(PERCENTILE(dur, 95)/1e6, 4) AS p95_ms,
  ROUND(PERCENTILE(dur, 99)/1e6, 4) AS p99_ms, ROUND(MAX(dur)/1e6, 4) AS max_ms
FROM slice WHERE name GLOB 'Lyrics.*' OR name = 'Playback.readClock'
GROUP BY name ORDER BY name;

SELECT 'frame schema' AS section;
PRAGMA table_info(actual_frame_timeline_slice);
SELECT 'app frames' AS section;
SELECT f.* FROM actual_frame_timeline_slice f JOIN process p USING(upid)
WHERE p.name = 'com.mocharealm.accompanist.demo' LIMIT 5;
SELECT 'main frame slice names' AS section;
SELECT s.name, s.ts, s.dur FROM slice s JOIN thread_track tt ON s.track_id = tt.id
JOIN thread th USING(utid) JOIN process p USING(upid)
WHERE p.name = 'com.mocharealm.accompanist.demo' AND th.is_main_thread
  AND s.name GLOB '*doFrame*' LIMIT 10;
SELECT 'trace losses and errors' AS section;
SELECT name, idx, value FROM stats WHERE value > 0 AND severity != 'info';
SELECT 'perf sample count' AS section;
SELECT COUNT(*) FROM perf_sample;

SELECT 'app frame completion and screen presentation' AS section;
WITH frames AS (
  SELECT a.ts, a.dur, a.jank_type, a.on_time_finish,
    e.ts AS expected_start, e.ts+e.dur AS expected_finish,
    sf.ts+sf.dur AS present_ts
  FROM actual_frame_timeline_slice a JOIN process p ON a.upid=p.upid
  JOIN expected_frame_timeline_slice e
    ON e.surface_frame_token=a.surface_frame_token AND e.upid=a.upid
  JOIN actual_frame_timeline_slice sf
    ON sf.display_frame_token=a.display_frame_token AND sf.surface_frame_token IS NULL
  WHERE p.name='com.mocharealm.accompanist.demo' AND a.dur>=0 AND sf.dur>=0
)
SELECT COUNT(*) AS frames, SUM(on_time_finish) AS app_finished_on_time,
  SUM(jank_type GLOB '*Buffer Stuffing*') AS buffer_stuffing_frames,
  ROUND(AVG(dur)/1e6,3) AS app_work_mean_ms,
  ROUND(PERCENTILE(dur,95)/1e6,3) AS app_work_p95_ms,
  ROUND(AVG(present_ts-ts)/1e6,3) AS frame_start_to_present_mean_ms,
  ROUND(PERCENTILE(present_ts-ts,50)/1e6,3) AS frame_start_to_present_p50_ms,
  ROUND(PERCENTILE(present_ts-ts,95)/1e6,3) AS frame_start_to_present_p95_ms,
  ROUND(AVG(present_ts-expected_finish)/1e6,3) AS presentation_lateness_mean_ms
FROM frames;

SELECT 'lyrics draw to presentation' AS section;
WITH doframes AS (
  SELECT s.ts,s.dur,
    CAST(SUBSTR(s.name,LENGTH('Choreographer#doFrame ')+1) AS INTEGER) AS token
  FROM slice s JOIN thread_track tt ON s.track_id=tt.id
  JOIN thread th USING(utid) JOIN process p USING(upid)
  WHERE p.name='com.mocharealm.accompanist.demo' AND th.is_main_thread
    AND s.name GLOB 'Choreographer#doFrame *'
), draws AS (
  SELECT f.token, MIN(r.ts) AS first_draw_ts, MAX(r.ts+r.dur) AS last_draw_end_ts,
    SUM(r.dur) AS cpu_draw_dur
  FROM doframes f JOIN slice r ON r.ts>=f.ts AND r.ts<f.ts+f.dur
  WHERE r.name='Lyrics.drawRow' OR r.name='Lyrics.drawPhonetics'
  GROUP BY f.token
), presented AS (
  SELECT d.*,sf.ts+sf.dur AS present_ts
  FROM draws d JOIN actual_frame_timeline_slice a ON a.surface_frame_token=d.token
  JOIN process p ON a.upid=p.upid
  JOIN actual_frame_timeline_slice sf
    ON sf.display_frame_token=a.display_frame_token AND sf.surface_frame_token IS NULL
  WHERE p.name='com.mocharealm.accompanist.demo' AND sf.dur>=0
)
SELECT COUNT(*) AS frames_with_lyrics_draw,
  ROUND(AVG(cpu_draw_dur)/1e6,3) AS lyrics_cpu_per_frame_mean_ms,
  ROUND(PERCENTILE(cpu_draw_dur,95)/1e6,3) AS lyrics_cpu_per_frame_p95_ms,
  ROUND(AVG(present_ts-first_draw_ts)/1e6,3) AS draw_to_present_mean_ms,
  ROUND(PERCENTILE(present_ts-first_draw_ts,50)/1e6,3) AS draw_to_present_p50_ms,
  ROUND(PERCENTILE(present_ts-first_draw_ts,95)/1e6,3) AS draw_to_present_p95_ms,
  ROUND(PERCENTILE(present_ts-first_draw_ts,99)/1e6,3) AS draw_to_present_p99_ms
FROM presented;
