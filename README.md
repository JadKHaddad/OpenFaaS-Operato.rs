# OpenFaaS Functions OperatoRS

## Install

* Install the CRD
```bash
openfaas_functions_operato_rs crd install
```

* Install the Operator in controller mode (in cluster)
```bash
openfaas_functions_operato_rs operator controller deploy install
```
If you would like to see, what exactly is being deployed, you can run the command 
```bash
openfaas_functions_operato_rs operator controller deploy print
```

* Now you can create a Function and wait for it to be ready
```bash
kubectl apply -f nodeinfo.yaml
kubectl wait --for=condition=ready openfaasfunctions -n openfaas-fn nodeinfo
```

## Run locally

* Run the Operator in controller mode
```bash
openfaas_functions_operato_rs operator controller run
```

## Run in docker

* Mount the kubeconfig file to the container
```bash
docker run --rm -it -v ~/.kube:/home/app/.kube openfaas_functions_operato_rs:latest operator controller run
```

```powershell
docker run --rm -it -v ${USERPROFILE}/.kube:/home/app/.kube openfaas_functions_operato_rs:latest operator controller run
```


## Uninstall

* Uninstall the CRD
```bash
openfaas_functions_operato_rs crd uninstall
```
Uninstalling the CRD will automatically delete all the Functions

* Uninstall the Operator
```bash
openfaas_functions_operato_rs operator controller deploy uninstall
```
Uninstalling the Operator will not delete the Functions. The Functions will no longer be managed by the Operator.

* Delete a Function
```bash
kubectl delete -f nodeinfo.yaml
```

## Help

```bash
openfaas_functions_operato_rs --help
```

## Notes

* The Function CRD is based on the ```OpenFaaS Function CRD``` with optional fields

## Rust version 

1.70.0

## Contributors

* Jad K. Haddad <jadkhaddad@gmail.com>

## License & copyright

© 2023 Jad K. Haddad
Licensed under the [Creative Commons Legal Code License](LICENSE)