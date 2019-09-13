# Searching Results

Thorium also allows users to search through tool results to find interesting
files. This is currently only available in the Web UI and can be accessed on
the home page. Thorium uses the [Lucene syntax](https://www.elastic.co/guide/en/kibana/current/lucene-query.html)
 for search queries. It is also important to remember that documents are
searched for a single group at a time. This means that for a document to be
returned all search parametes must be met by at least one group.

The following are some examples:

### Examples

Querying for results containing the text `pe32`:

```
pe32
````

Querying for results containing `pe32` or `Microsoft`:

```
pe32 OR Microsoft
```

Querying for results containing `rust` and `x86_64`:
```
rust AND x86_64
```

Querying for results containing the string `rust and x86_64`. Use quotes to
wrap search queries that contain white space or conditional keywords:
```
"rust and x86_64"
```

Querying for results containing the string `rust and x86_64` and `pe32`:
```
"rust and x86_64" AND pe32
```

Querying for results containing `pe32` or string `rust and x86_64` and `pe32`:
```
pe32 OR ("rust and x86_64" AND pe32)
```

Querying for results where a field named `PEType` is set to `"PE32+"`

```
"PEType:\"PE32+\""
```

### FAQ

##### Why does it take some time for tool results to become searchable?

It can take some time (usually < 10 seconds) for results to be searchable in 
Thorium because they are indexed asynchronusly. Thorium has a component called
the search-streamer that is responsible for tailing recent results and
streaming then into Elastic Search.

##### What does it mean that documents are search for a single group at a time?

Due to Thorium's permissioning requirements and how elastic operates each group
has its own document with results for a specific sample or repo. This means
that each group must meet all requirements for to be returned.

An example of this would be the following query returning only sample 1's results:

```
Query: "Corn:\"IsGood\"" AND "Fliffy:\"IsAGoodDog\""

Sample 1: {"Corn": "IsGood", "HasTaste": true, Fliffy": "IsAGoodDog", "group": "CoolKids"}
Sample 2: {"Corn": "IsBad", "HasTaste": false, "Fliffy": "IsAGoodDog", "group": "SadKids"}
````
