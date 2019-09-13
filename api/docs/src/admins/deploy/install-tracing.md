## Deploy Tracing (Jaeger + Quickwit)

### 1) Deploy Postgres DB (using Kubegres)

Quickwit will need a Postgres database to store metadata. This guide uses the Kubegres operator to deploy
a distributed instance of Postgres. Using Kubegres is optional, any Postgres deployment method may be used
included external (to K8s) options.

#### Deploy Kubegres CRD and operator

```bash
kubectl apply -f https://raw.githubusercontent.com/reactive-tech/kubegres/refs/tags/v1.19/kubegres.yaml
kubectl rollout status --watch --timeout=600s deployment.apps/kubegres-controller-manager -n kubegres-system
```

#### Create PostgresDB user password secrets

Update `SUPER_USER_PASSWORD` and `REPLICATION_PASSWORD` with secure values and save those to
put in the Quickwit helm values YAML file later in this guide.

```bash,editable
kubectl create ns quickwit
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Secret
metadata:
  name: postgres-cluster-auth
  namespace: quickwit
type: Opaque
stringData:
  superUserPassword: <SUPER_USER_PASSWORD>
  replicationUserPassword: <REPLICATION_PASSWORD>
EOF
```

#### Create a Kubegres postgres DB

Use the following command to deploy a Postgres cluster using Kubegres. It may be necessary to edit
the DB size, Postgres version, and `storageClassName` depending on the deployment environment.

```bash,editable
cat <<EOF | kubectl apply -f -
apiVersion: kubegres.reactive-tech.io/v1
kind: Kubegres
metadata:
  name: postgres
  namespace: quickwit
spec:
   replicas: 3
   image: docker.io/postgres:17
   database:
      storageClassName: csi-rbd-sc
      size: 4Ti
   env:
      - name: POSTGRES_PASSWORD
        valueFrom:
           secretKeyRef:
              name: postgres-cluster-auth
              key: superUserPassword
      - name: POSTGRES_REPLICATION_PASSWORD
        valueFrom:
           secretKeyRef:
              name: postgres-cluster-auth
              key: replicationUserPassword
EOF
```

#### Set password for Postgres Quickwit user role

After Kubegres has completed deployment of Postgres, create a Quickwit Postgres user role using the
following command. Before running the command, update `INSECURE_QUICKWIT_PASSWORD` to a secure value.

```bash,editable
kubectl rollout status --watch --timeout=600s statefulset/postgres-1 -n quickwit
kubectl -n quickwit exec -it pod/postgres-1-0 -- /bin/bash -c "PGPASSWORD=INSECURE_QUICKWIT_PASSWORD su postgres -c \"createdb quickwit-metastore\""
```

### 4) Deploy Quickwit

#### Add the Quickwit Helm repo

```bash
helm repo add quickwit https://helm.quickwit.io
helm repo update quickwit
```

#### Create a Quickwit Helm values config: `quickwit-values.yml`

Update the `POSTGRES_PASSWORD`, `ACCESS_ID`, and `SECRET_KEY` values before deploying Quickwit.
For non-rook deployments, the `endpoint` may also need to be updated to point at the correct S3
endpoint. Edit the hostname om `QW_METASTORE_URI` for Postgres instances that were not setup using
Kubegres.

```yaml,editable
image:
  repository: docker.io/quickwit/quickwit
  pullPolicy: IfNotPresent
  # Overrides the image tag whose default is the chart appVersion.
  #tag: v0.6.4
metastore:
  replicaCount: 1
  # Extra env for metastore
  extraEnv:
    QW_METASTORE_URI: "postgres://postgres:<POSTGRES_PASSWORD>@postgres.quickwit.svc.cluster.local:5432/quickwit-metastore"
config:
  default_index_root_uri: s3://quickwit/quickwit-indexes
  storage:
    s3:
      flavor: minio
      region: default
      endpoint: http://rook-ceph-rgw-thorium-s3-store.rook-ceph.svc.cluster.local
      force_path_style_access: true
      access_key_id: "<ACCESS_ID>"
      secret_access_key: "<SECRET_KEY>"
```

#### Now use that values file to install Quickwit

```bash
helm install quickwit quickwit/quickwit -n quickwit -f quickwit-values.yml
```

#### Verify Quickwit pods are all running

```bash
kubectl get pods -n quickwit
# NAME                                      READY   STATUS    RESTARTS   AGE
# postgres-2-0                              1/1     Running   0          1h
# postgres-3-0                              1/1     Running   0          1h
# postgres-4-0                              1/1     Running   0          1h
# quickwit-control-plane-HASH               1/1     Running   0          1h
# quickwit-indexer-0                        1/1     Running   0          1h
# quickwit-janitor-HASH                     1/1     Running   0          1h
# quickwit-metastore-HASH                   1/1     Running   0          1h
# quickwit-searcher-0                       1/1     Running   0          1h
# quickwit-searcher-1                       1/1     Running   0          1h
# quickwit-searcher-2                       1/1     Running   0          1h
```

### 5) Deploy Jaeger

#### Create a namespace for Jaeger

```bash
kubectl create ns jaeger
```

#### Create the Jaeger Statefulset

```bash,editable
cat <<EOF | kubectl apply -f -
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: jaeger
  namespace: jaeger
  labels:
    app: jaeger
spec:
  serviceName: jaeger
  replicas: 1
  selector:
    matchLabels:
      app: jaeger
  template:
    metadata:
      labels:
          app: jaeger
    spec:
      containers:
        - name: jaeger
          image: jaegertracing/jaeger-query:latest
          imagePullPolicy: Always
          env:
            - name: SPAN_STORAGE_TYPE
              value: "grpc"
            - name: GRPC_STORAGE_SERVER
              value: "quickwit-searcher.quickwit.svc.cluster.local:7281"
          resources:
            requests:
              memory: "8Gi"
              cpu: "2"
            limits:
              memory: "8Gi"
              cpu: "2"
EOF
```

#### Create the Jaeger service

```bash,editable
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Service
metadata:
  name: jaeger
  namespace: jaeger
spec:
  type: ClusterIP
  selector:
    app: jaeger
  ports:
  - name: jaeger
    port: 16686
    targetPort: 16686
EOF
```

#### Verify the Jaeger pod is running

```bash
kubectl get pods -n jaeger
# NAME       READY   STATUS    RESTARTS   AGE
# jaeger-0   1/1     Running   0          1h
```