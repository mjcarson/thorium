kubectl get secret -n thorium registry-passwd -o jsonpath='{.data.htpasswd}' | base64 --decode > htpasswd
kubectl get secret -n thorium registry-conf -o jsonpath='{.data.config\.yml}' | base64 --decode > config.yml
