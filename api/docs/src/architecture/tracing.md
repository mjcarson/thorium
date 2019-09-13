# Tracing/Logging

Thorium leverage tracing to accomplish logging. Tracing is very similar to
logging but with several advantages:

- Unified trace/log viewing/aggregation
- Traces provide richer information then conventional logs

##### Unified Tracing

With conventional logging you are logging to a central file server or to disk
(unless you feed your logs to elastic or another service). This means that when
a problem occurs you may have to find the node that a service was running on to
look at the logs. Then if the problem spans multiple nodes your are looking
across multiple nodes trying to correlate logs. This is greatly exacerbated in
Kubernetes as if an error takes down a pod then its logs can also be lost.

By leveraging tracing however we can log to both stdout and to a trace
collector at once. This means that admins can look at logs normally but can
also use the [Jaeger](https://www.jaegertracing.io/) webUI to view traces for
all services in Thorium. Jaeger allows for admins to search for tags by any
of the logged fields or by span type. This makes it much easier to locate
problems in Thorium. 

##### Richer Information

Exposing tracing in a webUI allows for much richer information to be exposed
compared to conventional logging. This is largely because you can minimize
what information is displayed at any given point unlike logs in a file. It also
retains the parent child relationship of events allowing you to see that some
action took place as part of a large action. The final aspect tracing provides
over traditional logging is timing information. You can see how long actions
take allowing you to find what operations are slow or causing problems.