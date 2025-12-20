interface GraphQLResponse<T> {
  data?: T;
  errors?: { message: string }[];
}

const defaultGraphqlUrl = 'http://localhost:8080/graphql';

export function getGraphqlUrl(): string {
  const envUrl = import.meta.env.VITE_GRAPHQL_URL as string | undefined;
  if (envUrl) {
    return envUrl;
  }

  if (typeof window !== 'undefined') {
    const url = new URL(window.location.origin);
    url.port = '8080';
    url.pathname = '/graphql';
    return url.toString();
  }

  return defaultGraphqlUrl;
}

export async function graphqlRequest<TData>(
  query: string,
  variables?: Record<string, unknown>
): Promise<TData> {
  const response = await fetch(getGraphqlUrl(), {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({ query, variables }),
  });

  if (!response.ok) {
    throw new Error(`GraphQL request failed with status ${String(response.status)}`);
  }

  const payload = (await response.json()) as GraphQLResponse<TData>;

  if (payload.errors?.length) {
    const message = payload.errors.map((error) => error.message).join('; ');
    throw new Error(message);
  }

  if (!payload.data) {
    throw new Error('GraphQL response did not include data');
  }

  return payload.data;
}
