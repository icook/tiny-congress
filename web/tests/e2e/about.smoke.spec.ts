import { expect, test } from './fixtures';

const BUILD_INFO_QUERY = `
  query BuildInfoQuery {
    buildInfo {
      version
      gitSha
      buildTime
    }
  }
`;
const API_URL = process.env.PLAYWRIGHT_API_URL ?? 'http://127.0.0.1:8080/graphql';

test('about page reflects API build info @smoke', async ({ page, request }) => {
  const apiResponse = await request.post(API_URL, { data: { query: BUILD_INFO_QUERY } });
  expect(apiResponse.ok()).toBeTruthy();

  interface BuildInfoPayload {
    data?: {
      buildInfo: {
        version: string;
        gitSha: string;
        buildTime: string;
      };
    };
  }

  const payload = (await apiResponse.json()) as BuildInfoPayload;
  const apiBuildInfo = payload.data?.buildInfo;
  expect(apiBuildInfo).toBeDefined();
  const buildInfo = apiBuildInfo!;

  // Build metadata should be baked into the Docker image, not defaults
  expect(buildInfo.gitSha, 'GIT_SHA not baked into image').not.toBe('unknown');
  expect(buildInfo.buildTime, 'BUILD_TIME not baked into image').not.toBe('unknown');
  expect(buildInfo.version, 'APP_VERSION not baked into image').not.toBe('dev');

  await page.goto('/about');

  // Wait for loading to finish (loading indicator disappears after API call completes)
  await expect(page.getByTestId('build-info-loading')).toBeHidden({ timeout: 15000 });

  // Verify no error is shown
  await expect(page.getByTestId('build-info-error')).toBeHidden();

  // Verify the displayed data matches what the API returned
  await expect(page.getByTestId('api-version')).toHaveText(buildInfo.version);
  await expect(page.getByTestId('api-git-sha')).toHaveText(buildInfo.gitSha);
  await expect(page.getByTestId('api-build-time')).toHaveText(buildInfo.buildTime);
});
