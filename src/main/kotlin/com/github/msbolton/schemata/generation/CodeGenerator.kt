package com.github.msbolton.schemata.generation

import com.github.msbolton.schemata.models.ClassModel
import java.io.File

interface CodeGenerator {
    fun generate(classModel: ClassModel, outputDir: File): File
}