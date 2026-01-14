When it comes to importing, your options are either to use the `quizzy import` command to copy a flashcard set from Quizlet or use `quizzy new` to create a new set based on the given file type.

### Importing from Quizlet
To import a flashcard set from Quizlet, use the following command:
```
quizzy import <optional-name> <optional-quizlet-set-url>
```
Replace `<quizlet-set-url>` with the actual URL of the Quizlet set you want to import. Quizzy will then provide you a link to open in a browser and from there you must save the file from your browser into a json file. Then provide the path to that file when prompted or call `quizzy import <name> <api-json-file>` with the path to the saved json file any time.

[!CAUTION]
`quizzy import` only works with json files formatted like the output from the generated Quizlet link, mixing up json formats between `quizzy import` and `quizzy new` will lead to errors.



### Creating a New Set from a File
You can create a new flashcard set from various file types using the `quizzy new` command. The supported file types include:
- CSV
- TSV (Tab-Separated Values)
- JSON
- TXT

When importing from json directly, the expected format is as follows:
```json
{
  "cards": [
    {"term": "Term 1", "definition": "Definition 1"},
    {"term": "Term 2", "definition": "Definition 2"},
    ... more cards ...
  ],
  ... ignored fields ...
}
```

For CSV and TSV files, the first column is treated as the term and the second column as the definition. TXT files are also assumed to be TSV.
```tsv
Term 1  Definition 1
Term 2  Definition 2
```
or
```csv
Term 1,Definition 1
Term 2,Definition 2
```
