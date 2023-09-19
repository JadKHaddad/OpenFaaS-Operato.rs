# OpenFaaS Functions OperatoRS

## Install
* Install the CRD
```bash
openfaas_functions_operato_rs crd install
```
* Install the Operator in controller mode
```bash
openfaas_functions_operato_rs operator controller deploy install
```
* Now you can create a Function
```bash
kubectl apply -f example-function.yaml
```

## Run locally
* Run the Operator in controller mode
```bash
openfaas_functions_operato_rs operator controller run
```

## Uninstall
* Uninstall the CRD
```bash
openfaas_functions_operato_rs crd uninstall
```
* Uninstall the Operator
```bash
openfaas_functions_operato_rs operator controller deploy uninstall
```
* Delete a Function
```bash
kubectl delete -f example-function.yaml
```

## Help
```bash
openfaas_functions_operato_rs --help
```

## Notes
* The Function CRD is based on the ```OpenFaaS Function CRD``` with optional fields