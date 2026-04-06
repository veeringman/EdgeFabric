{{/*
Expand the name of the chart.
*/}}
{{- define "edgefabric.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "edgefabric.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Common labels.
*/}}
{{- define "edgefabric.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- include "edgefabric.selectorLabels" . }}
{{- end }}

{{/*
Selector labels.
*/}}
{{- define "edgefabric.selectorLabels" -}}
app.kubernetes.io/name: {{ include "edgefabric.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
API selector labels.
*/}}
{{- define "edgefabric.apiSelectorLabels" -}}
{{- include "edgefabric.selectorLabels" . }}
app.kubernetes.io/component: api
{{- end }}

{{/*
Agent selector labels.
*/}}
{{- define "edgefabric.agentSelectorLabels" -}}
{{- include "edgefabric.selectorLabels" . }}
app.kubernetes.io/component: agent
{{- end }}

{{/*
ServiceAccount name.
*/}}
{{- define "edgefabric.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "edgefabric.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}
