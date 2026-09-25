-- Moving the app folder used to store install job states as binary JSON,
-- which the install job readers can't decode ("invalid utf-8 sequence").
UPDATE install_jobs SET state = json(state) WHERE typeof(state) = 'blob';
