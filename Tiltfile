docker_build(
    'tc/frontend-dev',
    '.',
    dockerfile='dockerfiles/Dockerfile.frontend-dev',
    live_update=[
        sync('./web', '/app/web'),
        run('cd /app && npm install', trigger=['./web/package.json', './web/package-lock.json'])
    ]
)
docker_build('tc/backend', '.', dockerfile='dockerfiles/Dockerfile.backend')

k8s_yaml(helm('kube/app', name='tc'))

# Port forwards for local development
k8s_resource('tc-frontend', port_forwards='3000:3000')
k8s_resource('tc', port_forwards='8080:8080')

# Label resources for better visibility
k8s_resource(
    'tc-frontend',
    labels=['frontend']
)

k8s_resource(
    'tc',
    labels=['backend']
)