package com.github.msbolton.schemata.cli

import picocli.CommandLine

@CommandLine.Command(
    name = "validate",
    description = ["Validates Schemata IDL files"]
)
class ValidateCommand : Runnable {

    @CommandLine.Option(names = ["-i", "--input"], description = ["Input .schema file"], required = true)
    lateinit var inputFile: String

    override fun run() {
        // Implement validation logic
        // Parse and validate the input .schema file
    }
}
