kubectl create secret generic operator-registry-token --dry-run=client --namespace="thorium" --type=kubernetes.io/dockerconfigjson --from-file=".dockerconfigjson" -o yaml > registry-token.yaml
kubectl apply -f registry-token.yaml
kubectl create secret tls api-certs --dry-run=client --namespace="thorium" --key="tls.key" --cert="tls.crt" -o yaml > api-certs.yaml
kubectl apply -f api-certs.yaml
#kubectl create secret generic kube-config --dry-run=client --namespace="thorium" --from-file="config" -o yaml > kube-config.yaml
#kubectl apply -f kube-config.yaml
