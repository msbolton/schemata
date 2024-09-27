package com.github.msbolton.schemata.cli

import io.micronaut.configuration.picocli.PicocliRunner
import picocli.CommandLine.Command

@Command(
    name = "schemata",
    description = ["Schemata CLI application"],
    mixinStandardHelpOptions = true,
    subcommands = [GenerateCommand::class, ValidateCommand::class]
)
class SchemataCommand : Runnable {

    override fun run() {
        // Default action if no subcommand is provided
        println("Schemata CLI - Use --help for more information.")
    }

    companion object {
        @JvmStatic
        fun main(args: Array<String>) {
            PicocliRunner.run(SchemataCommand::class.java, *args)
        }
    }
}
