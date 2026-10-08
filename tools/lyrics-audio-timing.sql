SELECT 'audio counters' AS section;
SELECT t.name, COUNT(*) AS samples, MIN(c.value) AS min_value,
  AVG(c.value) AS mean_value, PERCENTILE(c.value,50) AS p50_value,
  PERCENTILE(c.value,95) AS p95_value, PERCENTILE(c.value,99) AS p99_value,
  MAX(c.value) AS max_value
FROM counter c JOIN counter_track t ON c.track_id=t.id
WHERE t.name GLOB 'Audio.*' AND t.name NOT GLOB '*PositionUs'
GROUP BY t.name ORDER BY t.name;

SELECT 'clock read CPU' AS section;
SELECT name,COUNT(*) AS samples,AVG(dur)/1e6 AS mean_ms,
  PERCENTILE(dur,95)/1e6 AS p95_ms,MAX(dur)/1e6 AS max_ms
FROM slice WHERE name IN ('Audio.readTrackClock','Playback.readClock') GROUP BY name;

SELECT 'warm audio samples (excluding first 5 s)' AS section;
SELECT t.name, COUNT(*) AS samples, ROUND(AVG(c.value)/1000,3) AS mean_ms,
  ROUND(PERCENTILE(c.value,50)/1000,3) AS p50_ms,
  ROUND(PERCENTILE(c.value,95)/1000,3) AS p95_ms,
  ROUND(PERCENTILE(c.value,99)/1000,3) AS p99_ms,
  ROUND(MIN(c.value)/1000,3) AS min_ms,ROUND(MAX(c.value)/1000,3) AS max_ms
FROM counter c JOIN counter_track t ON c.track_id=t.id CROSS JOIN trace_bounds b
WHERE t.name IN ('Audio.headAheadOfHardwareUs','Audio.sinkMinusHardwareUs',
  'Audio.sinkMinusLyricsUs','Audio.sinkMinusControllerUs','Audio.timestampAgeUs',
  'Audio.hardwareMinusLyricsUs','Audio.hardwareMinusControllerUs',
  'Audio.sinkSampleAgeUs') AND c.ts>=b.start_ts+5000000000
GROUP BY t.name ORDER BY t.name;

SELECT 'trace status' AS section;
SELECT name,idx,value FROM stats WHERE value>0 AND severity!='info';
