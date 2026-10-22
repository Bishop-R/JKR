gfx/effects/raceShader
{
	{
	map gfx/mp/forceshell
	blendFunc GL_ONE GL_ONE
    rgbGen const ( 0.1 0.1 1 )
	//rgb num divided by 255
	tcGen environment
	tcMod rotate 25
	tcMod turb 0.4 0.2 0 0.4
	tcMod scale 0.2 0.2
	}
	/*{ 
		map textures/colors/red 
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA 
		depthWrite 
		rgbGen lightingDiffuse 
		alphaGen wave sin 0.1 0.1 0.1 0.1 
	}
	{ 
		map textures/colors/blue 
		blendFunc GL_ONE GL_ONE 
		rgbGen wave sin 0.9 0.1 0.1 0.1 
	}*/
}

gfx/effects/duelShader
{
	{
		map gfx/mp/forceshell
		blendFunc GL_ONE GL_ONE
		rgbGen const ( 0.1 0.1 0.4 )
		//rgb num divided by 255
		tcGen environment
		tcMod rotate 25
		tcMod turb 0.4 0.2 0 0.4
		tcMod scale 0.2 0.2
	}
	{ 
		map textures/colors/red 
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA 
		depthWrite 
		rgbGen lightingDiffuse 
		alphaGen wave sin 0.1 0.1 0.1 0.1 
	}
	{ 
		map textures/colors/blue 
		blendFunc GL_ONE GL_ONE 
		rgbGen wave sin 0.9 0.1 0.1 0.1 
	}
	/*{
		map gfx/effects/plasma
		blendFunc GL_DST_COLOR GL_ONE
		rgbGen entity
		tcGen environment
		tcMod scroll 0.3 0.2
		tcMod turb 0.6 0.3 0 0.2
	}*/
}
