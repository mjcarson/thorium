kubectl get secret -n thorium api-certs -o jsonpath='{.data.tls\.crt}' | base64 --decode > tls.crt
kubectl get secret -n thorium api-certs -o jsonpath='{.data.tls\.key}' | base64 --decode > tls.key
kubectl get secret -n thorium thorium -o jsonpath='{.data.thorium\.yml}' | base64 --decode > thorium.yml
kubectl get secret -n thorium keys -o jsonpath='{.data.keys\.yml}' | base64 --decode > keys.yml
#kubectl get secret -n thorium kube-config -o jsonpath='{.data.config}' | base64 --decode > config
kubectl get secret -n thorium docker-skopeo -o jsonpath='{.data.config\.json}' | base64 --decode > config.json
kubectl get secret -n thorium registry-token -o jsonpath='{.data.\.dockerconfigjson}' | base64 --decode > .dockerconfigjson
kubectl get secret -n thorium operator-registry-token -o jsonpath='{.data.\.dockerconfigjson}' | base64 --decode > .dockerconfigjson
