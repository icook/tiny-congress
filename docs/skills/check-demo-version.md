# check demo version

Check the deployed version of the TinyCongress demo against the current master branch.

Use this when the demo seems stale, after a deploy, or to verify a fix has rolled out.

## Steps

### 1. Fetch deployed build info

```bash
curl -sf https://demo-api.tinycongress.com/api/v1/build-info
```

Response shape:
```json
{
  "version": "master@<sha>",
  "gitSha": "<full 40-char sha>",
  "buildTime": "<RFC 3339 timestamp>"
}
```

### 2. Compare against remote master

```bash
git fetch origin master --quiet
git log --oneline origin/master | head -10
```

### 3. Count commits behind

Using the `gitSha` from step 1:

```bash
git log --oneline <deployed_sha>..origin/master
```

### 4. Report

- Deployed SHA and build time
- Current master HEAD
- How many commits behind (0 = current)
- If behind, list the commits that haven't deployed yet
