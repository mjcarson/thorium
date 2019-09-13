kubectl create secret generic registry-passwd --dry-run=client --namespace="thorium" --from-file="htpasswd" -o yaml > registry-htpasswd.yaml
kubectl apply -f registry-htpasswd.yaml
kubectl create secret generic registry-conf --dry-run=client --namespace="thorium" --from-file="config.yml" -o yaml > registry-conf.yaml 
kubectl apply -f registry-conf.yaml
